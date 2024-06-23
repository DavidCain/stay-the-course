use chrono::{DateTime, Datelike, Local};
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{params, Connection, NO_PARAMS};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::convert::Into;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

use crate::assets;
use crate::config::Config;
use crate::dateutil;
use crate::decutil;
use crate::quote;
use crate::rebalance::{AssetAllocation, Portfolio};

trait GnucashFromXML {
    fn from_xml(_: &mut Reader<BufReader<File>>) -> Self;
}

trait GnucashFromSqlite {
    fn from_sqlite(_: &Connection, conf: &Config) -> Self;
}

#[derive(Debug)]
pub struct CommodityError {
    pub commodity_id: String,
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

    fn at_new_quoted_value(&self, q: &quote::Quote) -> Price {
        Price {
            from_commodity: self.from_commodity.clone(),
            to_commodity: self.to_commodity.clone(),
            value: q.last,
            time: q.time,
        }
    }

    /**
     * Return if this quote has information not recorded in the latest price.
     *
     * Even if the value differs from what we have in the price, we should
     * still write it to the database anyway - GnuCash can pick which it prefers.
     */
    fn should_update_with_quote(&self, q: &quote::Quote) -> bool {
        self.time.date_naive() < q.time.date_naive() || (self.value != q.last)
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
                        found_ts = Some(dateutil::localize_from_dt_with_tz(&text).unwrap());
                    }
                    b"price:value" => {
                        let frac = reader.read_text(e.name(), &mut Vec::new()).unwrap();
                        value = decutil::frac_to_quantity(&frac).unwrap();
                    }
                    _ => (),
                },
                Ok(Event::End(ref e)) => {
                    if let b"price" = e.name() {
                        break;
                    }
                }
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

pub fn new_uuid() -> String {
    (*uuid::Uuid::new_v4()
        .to_simple()
        .encode_lower(&mut uuid::Uuid::encode_buffer()))
    .to_string()
}

impl PriceDatabase {
    fn new() -> PriceDatabase {
        let last_price_by_commodity: HashMap<String, Price> = HashMap::new();
        PriceDatabase {
            last_price_by_commodity,
        }
    }

    // TODO: Update the database in-place by using mut self
    pub fn write_price_from_quote(
        &self,
        conn: &Connection,
        q: &quote::Quote,
        old_price: &Price,
    ) -> Result<Price, CommodityError> {
        let new_price = old_price.at_new_quoted_value(q);
        let new_price_uuid = new_uuid();

        // Handle the edge case of commodities IDs being missing
        // (This should only happen if parsing from XML)
        let commodity_guid: String = match &new_price.from_commodity.guid {
            Some(guid) => guid.clone(),
            None => {
                return Err(CommodityError {
                    commodity_id: new_price.from_commodity.id.clone(),
                })
            }
        };
        let currency_guid: String = match &new_price.to_commodity.guid {
            Some(guid) => guid.clone(),
            None => {
                return Err(CommodityError {
                    commodity_id: new_price.to_commodity.id.clone(),
                })
            }
        };

        let cents: u64 = decutil::price_to_cents(&new_price.value).unwrap();

        conn.execute(
            "INSERT INTO prices (
                   guid,
                   commodity_guid,
                   currency_guid,

                   -- Actually a datestring! Warning: UTC, but where we always use noon *local* time
                   date,
                   source,
                   type,

                   value_num,
                   value_denom
               )
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &new_price_uuid,
                &commodity_guid,
                &currency_guid,
                &dateutil::datetime_for_sqlite(new_price.time),
                "Finance::Quote",
                "last",
                &cents.to_string(),
                "100",
            ],
        )
        .unwrap();

        Ok(new_price)
    }

    fn read_price(&mut self, price: Price) {
        let name = String::from(price.commodity_name());
        if let Some(existing) = self.last_price_by_commodity.get(&name) {
            if price.time < existing.time {
                return;
            }
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

    fn populate_from_sqlite(&mut self, conn: &Connection) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(
            "-- NOTE: This query uses a quirk of SQLite that does not comply with the SQL standard
                      -- (SQLite lets you `GROUP BY` columns, then select non-aggregate columns)
                      -- It's handy here, but it may not be portable to other SQL implementations
                      SELECT -- Fraction which forms the actual price
                             p.value_num, p.value_denom,

                             -- Last known price date
                             max(p.date),

                             -- Commodity for which the price is being quoted
                             from_c.guid, from_c.mnemonic, from_c.namespace, from_c.fullname,

                             -- Commodity in which the price is defined (generally a currency)
                             to_c.guid, to_c.mnemonic, to_c.namespace, to_c.fullname
                        FROM prices p
                             JOIN commodities from_c ON p.commodity_guid = from_c.guid
                             JOIN commodities to_c   ON p.currency_guid = to_c.guid
                       WHERE from_c.namespace IN ('FUND', 'Series I')
                       GROUP BY p.commodity_guid;",
        )?;

        let price_iter = stmt.query_map(NO_PARAMS, |row| {
            let num: i64 = row.get(0)?;
            let denom: i64 = row.get(1)?;
            let value: Decimal = Decimal::from(num) / Decimal::from(denom);

            let dt: String = row.get(2)?;

            let price = Price {
                value,
                time: dateutil::utc_to_datetime(&dt),
                from_commodity: Commodity::new(
                    Some(row.get(3)?),
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ),
                to_commodity: Commodity::new(
                    Some(row.get(7)?),
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ),
            };
            Ok(price)
        })?;
        for price in price_iter {
            self.read_price(price.unwrap());
        }
        Ok(())
    }

    fn populate_from_xml(&mut self, reader: &mut Reader<BufReader<File>>) {
        let mut buf = Vec::new();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    if let b"price" = e.name() {
                        let price = Price::from_xml(reader);
                        if !&price.is_in_usd() {
                            continue;
                        }
                        self.read_price(price);
                    }
                }
                Ok(Event::End(ref e)) => {
                    if let b"gnc:pricedb" = e.name() {
                        break;
                    }
                }
                Ok(_) => (),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            }
            buf.clear();
        }
    }
}

#[derive(Debug, Clone)]
pub struct Commodity {
    pub guid: Option<String>, // a UUID lowercased with no hypens, absent from XML
    pub id: String,           // "VTSAX"
    pub space: Option<String>, // "FUND", "CURRENCY", etc.
    pub name: String,         // "Vanguard Total Stock Market Index Fund"
}

impl Commodity {
    // Initialize with a potentially empty name
    fn new(
        guid: Option<String>,
        id: String,
        space: Option<String>,
        name: Option<String>,
    ) -> Commodity {
        Commodity {
            guid,
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
            Some(id) => Commodity::new(None, id, space, name),
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
            Ok(frac) => decutil::frac_to_quantity(&frac).unwrap(),
            Err(_) => panic!("Error parsing quantity"),
        }
    }

    #[allow(dead_code)]
    fn get_value(&self) -> Decimal {
        match &self.value_fraction {
            Ok(frac) => decutil::frac_to_quantity(&frac).unwrap(),
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
                Ok(Event::End(ref e)) => {
                    if let b"trn:split" = e.name() {
                        break;
                    }
                }
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
        dateutil::localize_from_dt_with_tz(&self.date_posted_string).unwrap()
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
                Ok(Event::End(ref e)) => {
                    if let b"trn:splits" = e.name() {
                        break;
                    }
                }
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
                Ok(Event::End(ref e)) => {
                    if let b"trn:date-posted" = e.name() {
                        break;
                    }
                }
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
                Ok(Event::End(ref e)) => {
                    if let b"gnc:transaction" = e.name() {
                        break;
                    }
                }
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

    fn read_splits_from_sqlite(&mut self, conn: &Connection) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(
            "SELECT account_guid,
                    value_num, value_denom,
                    quantity_num, quantity_denom
               FROM splits
              WHERE account_guid = $1
              ",
        )?;

        let splits = stmt.query_map([&self.guid].iter(), |row| {
            let account: String = row.get(0)?;

            let value_num: i64 = row.get(1)?;
            let value_denom: i64 = row.get(2)?;
            let value: Decimal = Decimal::from(value_num) / Decimal::from(value_denom);

            let quantity_num: i64 = row.get(3)?;
            let quantity_denom: i64 = row.get(4)?;
            let quantity: Decimal = Decimal::from(quantity_num) / Decimal::from(quantity_denom);

            let split = ComputedSplit {
                value,
                quantity,
                account,
            };
            Ok(split)
        })?;

        self.splits = splits
            .map(|split| Split::Computed(split.unwrap()))
            .collect();
        Ok(())
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
                Ok(Event::End(ref e)) => {
                    if let b"gnc:account" = e.name() {
                        break;
                    }
                }
                Ok(Event::Eof) => panic!("Unexpected EOF before closing account tag!"),
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
            buf.clear();
        }

        Account::new(guid, name, commodity)
    }
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
            Book::from_sqlite_file(path, conf)
        } else if conf.gnucash.file_format == "xml" {
            Book::from_xml_file(path)
        } else {
            panic!("Other file formats not supported at this time");
        }
    }

    pub fn from_sqlite_file(filename: &str, conf: &Config) -> Book {
        let conn = Connection::open(filename).expect("Could not open file");
        Book::from_sqlite(&conn, conf)
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
        if let Some(account) = self.account_by_guid.get_mut(&account_name) {
            account.add_split(split);
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
                .unwrap_or_else(|| panic!("No last price found for {:?}", account.commodity));

            let value = account.current_value(last_price);
            if value == 0.into() {
                // We ignore empty accounts
                continue;
            }

            let symbol: Option<String> = match &account.commodity {
                Some(commodity) => Some(commodity.id.to_owned()),
                None => None,
            };

            if let Some(commodity) = &account.commodity {
                let asset_class = asset_classifications.classify(&commodity.id).unwrap();
                non_zero_holdings.push(assets::Asset::new(
                    account.name.to_owned(),
                    symbol,
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

    fn alphavantage_commodities(conn: &Connection) -> rusqlite::Result<Vec<Commodity>> {
        let mut stmt = conn
            .prepare(
                "SELECT guid, mnemonic, namespace, fullname
                   FROM commodities
                  WHERE namespace = 'FUND'
                    AND quote_flag
                    AND quote_source = 'alphavantage'
                  ",
            )
            .expect("Invalid SQL");

        let commodities = stmt.query_map(NO_PARAMS, |row| {
            Ok(Commodity::new(
                Some(row.get(0)?),
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })?;

        Ok(commodities.map(|ret| ret.unwrap()).collect())
    }

    fn commodities_needing_quotes(&self, conn: &Connection) -> Vec<Commodity> {
        let now = Local::now();

        struct PriceAndCommodity<'a> {
            price: Option<&'a Price>,
            commodity: Commodity,
        }

        let mut commodities_and_prices: Vec<PriceAndCommodity> =
            Book::alphavantage_commodities(conn)
                .unwrap()
                .into_iter()
                .map(|commodity| PriceAndCommodity {
                    price: self.pricedb.last_commodity_price(&commodity),
                    commodity,
                })
                .filter(|cap| {
                    match cap.price {
                        Some(price) => {
                            let days = (now - price.time).num_days().abs();
                            // println!("Days without quote for {:}: {:}", cap.commodity.id, days);
                            match now.weekday() {
                                // (If it's currently the weekend, last Friday's fetch will do)
                                chrono::Weekday::Sat => days > 1,
                                chrono::Weekday::Sun => days > 2,
                                // On weekdays, settle for yesterday's quotes.
                                // (AlphaVantage's free API isn't always the most current)
                                _ => days > 1,
                            }
                        }
                        // If no price was found, we definitely need a new quote.
                        None => true,
                    }
                })
                .collect();

        // Commodities with the oldest date will come first
        commodities_and_prices.sort_by_key(|cap| match cap.price {
            Some(price) => price.time.date_naive(),
            // Because we can't currently handle them, put commodities missing prices last
            None => now.date_naive(),
        });
        commodities_and_prices
            .into_iter()
            .map(|cap| cap.commodity)
            .collect()
    }

    // TODO: Run these requests in parallel.
    fn update_price_if_needed(
        &self,
        conn: &Connection,
        commodity: &Commodity,
    ) -> Result<Option<Price>, quote::FinanceQuoteError> {
        let last_price = self.pricedb.last_commodity_price(commodity);

        // Output what's happening, since this can be slow.
        print!("Fetching latest price for {:}", commodity.id);
        if let Some(price) = last_price {
            print!(": {:}", price.value);
        }
        std::io::stdout().flush().ok();

        let last_quote = match quote::FinanceQuote::fetch_quote(commodity) {
            Ok(quote) => {
                println!(
                    " --> {:} ({:})",
                    quote.last,
                    quote.time.date_naive().format("%Y-%m-%d")
                );
                quote
            }
            Err(e) => {
                println!("  ERROR!");
                return Err(e);
            }
        };

        let updated_price: Option<Price> = match last_price {
            Some(price) => {
                if price.should_update_with_quote(&last_quote) {
                    self.pricedb
                        .write_price_from_quote(conn, &last_quote, &price)
                        .ok()
                } else {
                    None
                }
            }
            // TODO: When there's no known last price, we should be able to get the `to_commodity`
            // (which is just USD) and write the first price to the database.
            // However, since we lack the commodity UUID, we can't write.
            // For now, the best workaround for new commodities is to fetch once in Gnucash.
            None => {
                println!("Currently not able to write first price on new commodities");
                None
            }
        };

        Ok(updated_price)
    }
    fn update_commodities(
        &self,
        conn: &Connection,
    ) -> Result<Vec<Price>, quote::FinanceQuoteError> {
        let mut new_prices = Vec::new();
        for commodity in self.commodities_needing_quotes(conn).iter() {
            if let Some(price) = self.update_price_if_needed(conn, &commodity)? {
                new_prices.push(price);
            }
        }
        Ok(new_prices)
    }

    fn get_accounts(conn: &Connection, namespace: &str) -> Vec<Account> {
        let mut stmt = conn
            .prepare(
                "SELECT a.guid, a.name,
                        -- Commodity for the account
                        c.guid, c.mnemonic, c.namespace, c.fullname
                   FROM accounts a
                        JOIN commodities c ON a.commodity_guid = c.guid
                  WHERE c.namespace = $1
                  ",
            )
            .expect("Invalid SQL");

        stmt.query_map([namespace], |row| {
            let account_guid = row.get(0)?;
            let account_name = row.get(1)?;
            let commodity =
                Commodity::new(Some(row.get(2)?), row.get(3)?, row.get(4)?, row.get(5)?);

            Ok(Account::new(account_guid, account_name, Some(commodity)))
        })
        .unwrap()
        .map(|ret| ret.unwrap())
        .collect()
    }
}

impl GnucashFromSqlite for Book {
    fn from_sqlite(conn: &Connection, conf: &Config) -> Book {
        let mut book = Book::new();

        for mut account in Book::get_accounts(conn, "FUND") {
            assert!(account.is_investment());
            account.read_splits_from_sqlite(conn).unwrap();
            book.add_investment(account);
        }

        // I Bonds are an interesting case -- they should count as bounds in any
        // portfolio, but they also aren't publicly-traded funds (nor is it easy
        // to fetch the current value of an I Bond).
        //
        // To get around all this, I make up ticker names for my I Bonds, then
        // just use the Price Editor to input the values from TreasuryDirect.gov
        // (every ~year or so, since interest rates are adjusted twice yearly).
        for mut account in Book::get_accounts(conn, "Series I") {
            account.read_splits_from_sqlite(conn).unwrap();
            book.add_investment(account);
        }

        book.pricedb.populate_from_sqlite(conn).unwrap();
        if conf.gnucash.update_prices {
            match book.update_commodities(conn) {
                Ok(updated_commodities) => {
                    if !updated_commodities.is_empty() {
                        // Currently, must re-populate from database to get the most current prices!
                        // TODO: `write_price_from_quote()` should update the PriceDatabase in-place
                        book.pricedb.populate_from_sqlite(conn).unwrap();
                    }
                }
                Err(e) => println!(
                    "Failed to fetch price for {:}, continuing without updating other prices",
                    e.symbol
                ),
            };
        }
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
