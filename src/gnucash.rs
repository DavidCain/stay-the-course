extern crate chrono;
extern crate quick_xml;
extern crate rusqlite;
extern crate rust_decimal;

use self::chrono::{DateTime, Local, NaiveDateTime, Utc};
use self::quick_xml::events::Event;
use self::quick_xml::Reader;
use self::rusqlite::{Connection, NO_PARAMS};
use self::rust_decimal::Decimal;
use std::collections::HashMap;
use std::convert::Into;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;

use assets;
use config::Config;
use quote;
use rebalance::{AssetAllocation, Portfolio};

static GNUCASH_DT_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
static GNUCASH_UTC_DT_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// In XML, datetimes are given with local TZ in them
fn to_datetime(datestring: &str) -> DateTime<Local> {
    let dt = DateTime::parse_from_str(datestring, GNUCASH_DT_FORMAT).unwrap();
    dt.with_timezone(&Local)
}

// In SQLite, all datetimes are UTC
fn utc_to_datetime(datestring: &str) -> DateTime<Local> {
    let dt = NaiveDateTime::parse_from_str(datestring, GNUCASH_UTC_DT_FORMAT).unwrap();
    let utc = DateTime::<Utc>::from_utc(dt, Utc);
    utc.with_timezone(&Local)
}

trait GnucashFromXML {
    fn from_xml(&mut Reader<BufReader<File>>) -> Self;
}

trait GnucashFromSqlite {
    fn from_sqlite(&Connection) -> Self;
}

#[derive(Debug)]
struct Price {
    from_commodity: Commodity,
    to_commodity: Commodity,
    value: Decimal,
    time: DateTime<Local>,
}

impl Price {
    fn is_in_usd(&self) -> bool {
        match &self.to_commodity.space {
            Some(space) => space == "CURRENCY" && self.to_commodity.id == "USD",
            None => false,
        }
    }

    fn commodity_name(&self) -> &str {
        self.from_commodity.id.as_ref()
    }
}

impl GnucashFromXML for Price {
    fn from_xml(reader: &mut Reader<BufReader<File>>) -> Price {
        let mut buf = Vec::new();

        let mut maybe_from_commodity = None;
        let mut maybe_to_commodity = None;
        let mut value: Decimal = 0.into();
        let mut found_ts = None;

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name() {
                    b"price:commodity" => {
                        maybe_from_commodity = Some(Commodity::from_xml(reader));
                    }
                    b"price:currency" => {
                        maybe_to_commodity = Some(Commodity::from_xml(reader));
                    }
                    b"ts:date" => {
                        let text = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                        found_ts = Some(to_datetime(&text));
                    }
                    b"price:value" => {
                        let frac = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                        value = frac_to_quantity(&frac);
                    }
                    _ => (),
                },
                Ok(Event::End(ref e)) => match e.name() {
                    b"price" => break,
                    _ => (),
                },
                Ok(_) => (),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            }
            buf.clear();
        }

        match (maybe_from_commodity, maybe_to_commodity, found_ts) {
            (Some(from_commodity), Some(to_commodity), Some(time)) => Price {
                from_commodity,
                to_commodity,
                value,
                time,
            },
            (Some(_), Some(_), None) => panic!("No timestamp found on price!"),
            (_, _, _) => panic!("Prices must have a to/from commodity and a timestamp"),
        }
    }
}

#[derive(Debug)]
struct PriceDatabase {
    last_price_by_commodity: HashMap<String, Price>,
}

impl PriceDatabase {
    fn new() -> PriceDatabase {
        let last_price_by_commodity: HashMap<String, Price> = HashMap::new();
        PriceDatabase {
            last_price_by_commodity,
        }
    }

    fn add_price(&mut self, price: Price) {
        let name = String::from(price.commodity_name());
        match self.last_price_by_commodity.get(&name) {
            Some(existing) => {
                if price.time < existing.time {
                    return;
                }
            }
            None => (),
        }
        self.last_price_by_commodity.insert(name, price);
    }

    fn last_commodity_price(&self, commodity: &Commodity) -> Option<&Price> {
        self.last_price_by_commodity.get(&commodity.id)
    }

    fn last_price_for(&self, account: &Account) -> Option<&Price> {
        match &account.commodity {
            Some(commodity) => self.last_commodity_price(&commodity),
            None => panic!("Can't fetch last price of an account without a commodity"),
        }
    }

    fn populate_from_sqlite(&mut self, conn: &Connection) {
        let mut stmt = conn
            .prepare("-- NOTE: This query uses a quirk of SQLite that does not comply with the SQL standard
                      -- (SQLite lets you `GROUP BY` columns, then select non-aggregate columns)
                      -- It's handy here, but it may not be portable to other SQL implementations
                      SELECT -- Fraction which forms the actual price
                             p.value_num, p.value_denom,

                             -- Last known price date
                             max(p.date),

                             -- Commodity for which the price is being quoted
                             from_c.mnemonic, from_c.namespace, from_c.fullname,

                             -- Commodity in which the price is defined (generally a currency)
                             to_c.mnemonic, to_c.namespace, to_c.fullname
                        FROM prices p
                             JOIN commodities from_c ON p.commodity_guid = from_c.guid
                             JOIN commodities to_c   ON p.currency_guid = to_c.guid
                       WHERE from_c.namespace = 'FUND'
                       GROUP BY p.commodity_guid;")
            .expect("Invalid SQL");

        let price_iter = stmt
            .query_map(NO_PARAMS, |row| {
                let num: i64 = row.get(0);
                let denom: i64 = row.get(1);
                let value: Decimal = Decimal::from(num) / Decimal::from(denom);

                let dt: String = row.get(2);

                let price = Price {
                    value,
                    time: utc_to_datetime(&dt),
                    from_commodity: Commodity::new(row.get(3), row.get(4), row.get(5)),
                    to_commodity: Commodity::new(row.get(6), row.get(7), row.get(8)),
                };
                price
            })
            .expect("Could not iterate over SQL results");
        for price in price_iter {
            self.add_price(price.unwrap());
        }
    }

    fn populate_from_xml(&mut self, reader: &mut Reader<BufReader<File>>) {
        let mut buf = Vec::new();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name() {
                    b"price" => {
                        let price = Price::from_xml(reader);
                        if !&price.is_in_usd() {
                            continue;
                        }
                        self.add_price(price);
                    }
                    _ => (),
                },
                Ok(Event::End(ref e)) => match e.name() {
                    b"gnc:pricedb" => break,
                    _ => (),
                },
                Ok(_) => (),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            }
            buf.clear();
        }
    }
}

#[derive(Debug)]
pub struct Commodity {
    pub id: String,            // "VTSAX
    pub space: Option<String>, // "FUND", "CURRENCY", etc.
    pub name: String,          // "Vanguard Total Stock Market Index Fund"
}

impl Commodity {
    // Initialize with a potentially empty name
    fn new(id: String, space: Option<String>, name: Option<String>) -> Commodity {
        Commodity {
            space,
            // Name can be missing. Fall back to an ID if we lack a name
            name: match name {
                Some(commodity_name) => commodity_name,
                None => id.clone(),
            },
            id,
        }
    }

    fn is_investment(&self) -> bool {
        match &self.space {
            Some(space) => space == "FUND",
            None => false,
        }
    }
}

impl GnucashFromXML for Commodity {
    fn from_xml(reader: &mut Reader<BufReader<File>>) -> Commodity {
        let mut buf = Vec::new();

        let mut space = None;
        let mut id = None;
        let mut name = None;

        loop {
            match reader.read_event(&mut buf) {
                // Stop at the top of all top-level tags that have content we care about
                Ok(Event::Start(ref e)) => match e.name() {
                    b"cmdty:space" => {
                        space = Some(reader.read_text(e.name(), &mut Vec::new()).unwrap());
                    }
                    b"cmdty:id" => {
                        id = Some(reader.read_text(e.name(), &mut Vec::new()).unwrap());
                    }
                    b"cmdty:name" => {
                        name = Some(reader.read_text(e.name(), &mut Vec::new()).unwrap());
                    }
                    _ => (),
                },
                // If we found the end of this commodity tag, then stop moving through the tree
                // (We don't want to progress into other tags)
                // (Doesn't handle nested tags, but that's okay - gnc:commodity never nests)
                Ok(Event::End(ref e)) => match e.name() {
                    b"price:currency" => break,
                    b"act:commodity" | b"gnc:commodity" | b"price:commodity" => break,
                    _ => (),
                },
                Ok(Event::Eof) => panic!("Unexpected EOF before closing commodity tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }

        match id {
            Some(id) => Commodity::new(id, space, name),
            _ => panic!("Commodities must have an ID!"),
        }
    }
}

trait GenericSplit {
    fn get_quantity(&self) -> Decimal;
    fn get_value(&self) -> Decimal;
}

// Simple split that can be used when we don't care to defer Decimal arithmetic
struct ComputedSplit {
    value: Decimal,
    quantity: Decimal,
    account: String, // guid
}

impl GenericSplit for ComputedSplit {
    fn get_quantity(&self) -> Decimal {
        self.quantity
    }

    fn get_value(&self) -> Decimal {
        self.value
    }
}

#[derive(Debug)]
struct LazySplit {
    // Parsing value & quantity into Decimal is expensive.
    // Don't bother if we don't need to.
    value_fraction: Result<String, quick_xml::Error>,
    quantity_fraction: Result<String, quick_xml::Error>,
    account: String, // guid
}

impl GenericSplit for LazySplit {
    fn get_quantity(&self) -> Decimal {
        match &self.quantity_fraction {
            Ok(frac) => frac_to_quantity(&frac),
            Err(_) => panic!("Error parsing quantity"),
        }
    }

    #[allow(dead_code)]
    fn get_value(&self) -> Decimal {
        match &self.value_fraction {
            Ok(frac) => frac_to_quantity(&frac),
            Err(_) => panic!("Error parsing value"),
        }
    }
}

impl Into<ComputedSplit> for LazySplit {
    fn into(self) -> ComputedSplit {
        ComputedSplit {
            value: self.get_value(),
            quantity: self.get_quantity(),
            account: self.account,
        }
    }
}

impl GnucashFromXML for LazySplit {
    fn from_xml(reader: &mut Reader<BufReader<File>>) -> LazySplit {
        let mut buf = Vec::new();

        let mut value_fraction = None;
        let mut quantity_fraction = None;
        let mut account = None;

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name() {
                    b"split:value" => {
                        value_fraction = Some(reader.read_text(e.name(), &mut Vec::new()));
                    }
                    b"split:quantity" => {
                        quantity_fraction = Some(reader.read_text(e.name(), &mut Vec::new()));
                    }
                    b"split:account" => {
                        account = Some(reader.read_text(e.name(), &mut Vec::new()).unwrap());
                    }
                    _ => (),
                },
                Ok(Event::End(ref e)) => match e.name() {
                    b"trn:split" => break,
                    _ => (),
                },
                Ok(_) => (),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            }
            buf.clear();
        }

        match (value_fraction, quantity_fraction, account) {
            (Some(value_fraction), Some(quantity_fraction), Some(account)) => LazySplit {
                value_fraction,
                quantity_fraction,
                account,
            },
            (_, _, _) => panic!("Must have value, quantity, and account in a split"),
        }
    }
}

enum Split {
    Computed(ComputedSplit),
    Lazy(LazySplit),
}

struct Transaction {
    #[allow(dead_code)]
    name: String,
    date_posted_string: String,
    splits: Vec<Split>,
}

impl Transaction {
    #[allow(dead_code)]
    fn date_posted(&self) -> DateTime<Local> {
        to_datetime(&self.date_posted_string)
    }

    fn parse_splits(reader: &mut Reader<BufReader<File>>) -> Vec<Split> {
        let mut splits: Vec<Split> = Vec::new();
        let mut buf = Vec::new();

        loop {
            match reader.read_event(&mut buf) {
                // Stop at the top of all top-level tags that have content we care about
                Ok(Event::Start(ref e)) => match e.name() {
                    b"trn:split" => {
                        splits.push(Split::Lazy(LazySplit::from_xml(reader)));
                    }
                    _ => panic!("Unexpected tag in list of splits"),
                },
                Ok(Event::End(ref e)) => match e.name() {
                    b"trn:splits" => break,
                    _ => (),
                },
                Ok(Event::Eof) => panic!("Unexpected EOF before closing splits tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }
        splits
    }

    fn parse_date_posted(reader: &mut Reader<BufReader<File>>) -> String {
        let mut buf = Vec::new();

        let mut found_ts = None;
        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => match e.name() {
                    b"ts:date" => {
                        found_ts = Some(reader.read_text(e.name(), &mut Vec::new()).unwrap());
                    }
                    _ => panic!("Unexpected tag in list of splits"),
                },
                Ok(Event::End(ref e)) => match e.name() {
                    b"trn:date-posted" => break,
                    _ => (),
                },
                Ok(Event::Eof) => panic!("Unexpected EOF before closing date-posted tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }
        match found_ts {
            Some(ts) => ts,
            None => panic!("No timestamp found"),
        }
    }
}

impl GnucashFromXML for Transaction {
    fn from_xml(reader: &mut Reader<BufReader<File>>) -> Transaction {
        let mut buf = Vec::new();

        let mut name: String = String::from("");
        let mut parsed_splits = None;
        let mut date_posted = None;

        loop {
            match reader.read_event(&mut buf) {
                // Stop at the top of all top-level tags that have content we care about
                Ok(Event::Start(ref e)) => match e.name() {
                    b"trn:date-posted" => {
                        date_posted = Some(Transaction::parse_date_posted(reader));
                    }
                    b"trn:name" => {
                        name = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                    }
                    b"trn:splits" => {
                        parsed_splits = Some(Transaction::parse_splits(reader));
                    }
                    _ => (),
                },
                // If we found the end of this commodity tag, then stop moving through the tree
                // (We don't want to progress into other tags)
                // (Doesn't handle nested tags, but that's okay - gnc:commodity never nests)
                Ok(Event::End(ref e)) => match e.name() {
                    b"gnc:transaction" => break,
                    _ => (),
                },
                Ok(Event::Eof) => panic!("Unexpected EOF before closing transaction tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }
        match (parsed_splits, date_posted) {
            (Some(splits), Some(date_posted_string)) => Transaction {
                name,
                date_posted_string,
                splits,
            },
            (Some(_), None) => panic!("Found a transaction with no date posted"),
            (None, Some(_)) => panic!("Found a transaction with no splits"),
            (None, None) => panic!("Found a transaction without splits or a date posted"),
        }
    }
}

struct Account {
    guid: String,
    name: String,

    // Some accounts, e.g. parent accounts or the ROOT account have no commodity
    commodity: Option<Commodity>,

    splits: Vec<Split>,
}

impl Account {
    fn new(guid: String, name: String, commodity: Option<Commodity>) -> Account {
        // Start with an empty vector, we'll mutate later
        let splits = Vec::new();
        Account {
            guid,
            name,
            commodity,
            splits,
        }
    }

    fn read_splits_from_sqlite(&mut self, conn: &Connection) {
        let mut stmt = conn
            .prepare(
                "SELECT account_guid,
                        value_num, value_denom,
                        quantity_num, quantity_denom
                   FROM splits
                  WHERE account_guid = $1
                  ",
            )
            .expect("Invalid SQL");

        let splits = stmt
            .query_map([&self.guid].iter(), |row| {
                let account: String = row.get(0);

                let value_num: i64 = row.get(1);
                let value_denom: i64 = row.get(2);
                let value: Decimal = Decimal::from(value_num) / Decimal::from(value_denom);

                let quantity_num: i64 = row.get(3);
                let quantity_denom: i64 = row.get(4);
                let quantity: Decimal = Decimal::from(quantity_num) / Decimal::from(quantity_denom);

                let split = ComputedSplit {
                    value,
                    quantity,
                    account,
                };
                split
            })
            .unwrap()
            .map(|ret| Split::Computed(ret.unwrap()))
            .collect();
        self.splits = splits;
    }

    fn is_investment(&self) -> bool {
        if let Some(ref commodity) = self.commodity {
            return commodity.is_investment();
        }
        false
    }

    fn add_split(&mut self, split: Split) {
        self.splits.push(split);
    }

    fn current_quantity(&self) -> Decimal {
        // std::iter::Sum<d128> isn't implemented. =(
        let mut total = 0.into();
        for split in self.splits.iter() {
            total += match split {
                Split::Lazy(lazy_split) => lazy_split.get_quantity(),
                Split::Computed(computed_split) => computed_split.get_quantity(),
            }
        }
        total
    }

    fn current_value(&self, last_known_price: &Price) -> Decimal {
        match &self.commodity {
            Some(commodity) => {
                if commodity.id != last_known_price.from_commodity.id {
                    panic!("Last known price is for a different commodity!")
                }
            }
            None => panic!("Can't assert value of an account without a commodity"),
        }
        self.current_quantity() * last_known_price.value
    }
}

impl GnucashFromXML for Account {
    fn from_xml(mut reader: &mut Reader<BufReader<File>>) -> Account {
        let mut buf = Vec::new();

        let mut guid: String = String::from("");
        let mut name: String = String::from("");
        let mut commodity = None;

        loop {
            match reader.read_event(&mut buf) {
                // Stop at the top of all top-level tags that have content we care about
                Ok(Event::Start(ref e)) => match e.name() {
                    b"act:id" => {
                        guid = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                    }
                    b"act:name" => {
                        name = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                    }
                    b"act:commodity" => {
                        commodity = Some(Commodity::from_xml(&mut reader));
                    }
                    _ => (),
                },
                // If we found the end of this account tag, then stop moving through the tree
                // (We don't want to progress into other tags)
                // (Doesn't handle nested tags, but that's okay - gnc:account never nests)
                Ok(Event::End(ref e)) => match e.name() {
                    b"gnc:account" => break,
                    _ => (),
                },
                Ok(Event::Eof) => panic!("Unexpected EOF before closing account tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }

        Account::new(guid, name, commodity)
    }
}

fn frac_to_quantity(fraction: &str) -> Decimal {
    let mut components = fraction.split("/");
    let numerator = components.next().unwrap();
    let denomenator = components.next().unwrap();
    Decimal::from_str(numerator).unwrap() / Decimal::from_str(denomenator).unwrap()
}

pub struct Book {
    pricedb: PriceDatabase,
    account_by_guid: HashMap<String, Account>,
}

impl Book {
    fn new() -> Book {
        Book {
            pricedb: PriceDatabase::new(),
            account_by_guid: HashMap::new(),
        }
    }

    pub fn from_config(conf: &Config) -> Book {
        let path = &conf.gnucash.path_to_book;
        if conf.gnucash.file_format == "sqlite3" {
            return Book::from_sqlite_file(path);
        } else if conf.gnucash.file_format == "xml" {
            return Book::from_xml_file(path);
        } else {
            panic!("Other file formats not supported at this time");
        }
    }

    pub fn from_sqlite_file(filename: &str) -> Book {
        let conn = Connection::open(filename).expect("Could not open file");
        Book::from_sqlite(&conn)
    }

    #[allow(dead_code)]
    pub fn from_xml_file(filename: &str) -> Book {
        println!("This can be sluggish on larger XML files. Consider SQLite format instead!");
        let mut reader = Reader::from_file(filename).unwrap();
        Book::from_xml(&mut reader)
    }

    fn add_split(&mut self, split: Split) {
        let account_name = match &split {
            Split::Lazy(lazy_split) => lazy_split.account.clone(),
            Split::Computed(computed_split) => computed_split.account.clone(),
        };
        match self.account_by_guid.get_mut(&account_name) {
            Some(account) => account.add_split(split),
            None => (),
        }
    }

    fn add_investment(&mut self, account: Account) {
        self.account_by_guid.insert(account.guid.clone(), account);
    }

    /// Return all investment holdings worth more than $0
    fn holdings(&self, asset_classifications: assets::AssetClassifications) -> Vec<assets::Asset> {
        let mut non_zero_holdings = Vec::new();
        for account in self.account_by_guid.values() {
            let last_price = self
                .pricedb
                .last_price_for(account)
                .expect(&format!("No last price found for {:?}", account.commodity));

            let value = account.current_value(last_price);
            if value == 0.into() {
                // We ignore empty accounts
                continue;
            }

            if let Some(commodity) = &account.commodity {
                let asset_class = asset_classifications.classify(&commodity.id).unwrap();
                non_zero_holdings.push(assets::Asset::new(
                    account.name.to_owned(),
                    value,
                    asset_class.to_owned(),
                    Some(account.current_quantity()),
                    Some(last_price.value),
                    Some(last_price.time),
                ));
            } else {
                panic!("Account lacks a commodity! This should not happen");
            }
        }
        non_zero_holdings
    }

    pub fn portfolio_status(
        &self,
        asset_classifications: assets::AssetClassifications,
        ideal_allocations: Vec<AssetAllocation>,
    ) -> Portfolio {
        let mut by_asset_class: HashMap<assets::AssetClass, AssetAllocation> = HashMap::new();
        for allocation in ideal_allocations.into_iter() {
            by_asset_class.insert(allocation.asset_class.clone(), allocation);
        }

        for asset in self.holdings(asset_classifications) {
            // We ignore asset types not included in allocation
            if let Some(allocation) = by_asset_class.get_mut(&asset.asset_class) {
                allocation.add_asset(asset);
            }
        }
        Portfolio::new(by_asset_class.into_iter().map(|(_, v)| v).collect())
    }

    fn alphavantage_commodities(conn: &Connection) -> Vec<Commodity> {
        let mut stmt = conn
            .prepare(
                "SELECT mnemonic, namespace, fullname
                   FROM commodities
                  WHERE namespace = 'FUND'
                    AND quote_flag
                    AND quote_source = 'alphavantage'
                  ",
            )
            .expect("Invalid SQL");

        let commodities = stmt
            .query_map(NO_PARAMS, |row| {
                Commodity::new(row.get(0), row.get(1), row.get(2))
            })
            .unwrap()
            .map(|ret| ret.unwrap())
            .collect();

        commodities
    }

    fn commodities_needing_quotes(&self, conn: &Connection) -> Vec<Commodity> {
        let now = Local::now();

        Book::alphavantage_commodities(conn)
            .into_iter()
            .filter(|commodity| {
                let price = self.pricedb.last_commodity_price(&commodity);
                match price {
                    // NOTE: It could be the weekend or a trading holiday
                    // Trying to get a quote might be fruitless, but that's okay.
                    // (We can get a quote, see that if it's stale, then try again tomorrow)
                    Some(price) => {
                        let days = (now - price.time).num_days().abs();
                        // println!("Days without quote for {:}: {:}", commodity.id, days);
                        days > 1
                    }
                    // If no price was found, we definitely need a new quote.
                    None => true,
                }
            })
            .collect()
    }

    pub fn update_commodities(&self, conn: &Connection) {
        // TODO: Time out after five?
        for commodity in self.commodities_needing_quotes(conn) {
            let quote = quote::FinanceQuote::fetch_quote(&commodity);
            let price = self.pricedb.last_commodity_price(&commodity);

            let should_update = match price {
                // Update if the quote is more current, or it's the same date but different info
                Some(price) => {
                    price.time.naive_local().date() < quote.time.date() || price.value != quote.last
                }
                None => false,
            };
        }
    }

    fn investment_accounts(conn: &Connection) -> Vec<Account> {
        let mut stmt = conn
            .prepare(
                "SELECT a.guid, a.name,
                        -- Commodity for the account
                        c.mnemonic, c.namespace, c.fullname
                   FROM accounts a
                        JOIN commodities c ON a.commodity_guid = c.guid
                  WHERE c.namespace = 'FUND'
                  ",
            )
            .expect("Invalid SQL");

        let investment_accounts = stmt
            .query_map(NO_PARAMS, |row| {
                let guid = row.get(0);
                let name = row.get(1);
                let commodity = Commodity::new(row.get(2), row.get(3), row.get(4));

                Account::new(guid, name, Some(commodity))
            })
            .unwrap()
            .map(|ret| ret.unwrap())
            .collect();

        investment_accounts
    }
}

impl GnucashFromSqlite for Book {
    fn from_sqlite(conn: &Connection) -> Book {
        let mut book = Book::new();

        for mut account in Book::investment_accounts(conn) {
            assert!(account.is_investment());
            account.read_splits_from_sqlite(conn);
            book.add_investment(account);
        }

        book.pricedb.populate_from_sqlite(conn);
        book.update_commodities(conn);
        book
    }
}

impl GnucashFromXML for Book {
    fn from_xml(reader: &mut Reader<BufReader<File>>) -> Book {
        let mut book = Book::new();

        let mut buf = Vec::new();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    match e.name() {
                        /* Don't bother parsing all commodities: We only care about ones in accounts.
                        b"gnc:commodity" => {
                        let commodity = Commodity::from_xml(&mut reader);
                        },
                        */
                        b"gnc:pricedb" => {
                            book.pricedb.populate_from_xml(reader);
                        }
                        // The account fields come before transactions
                        b"gnc:account" => {
                            let account = Account::from_xml(reader);
                            if account.is_investment() {
                                book.add_investment(account);
                            }
                        }
                        // By the time we've reached this section, we've parsed all accounts.
                        b"gnc:transaction" => {
                            let transaction = Transaction::from_xml(reader);
                            for split in transaction.splits.into_iter() {
                                book.add_split(split);
                            }
                        }
                        _ => (),
                    }
                }
                Ok(Event::Eof) => break, // exits the loop when reaching end of file
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }

            // if we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
            buf.clear();
        }

        book
    }
}
