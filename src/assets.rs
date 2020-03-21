use chrono::{DateTime, Local};
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclassifiedAssetError {
    fund_name: String,
}

impl UnclassifiedAssetError {
    fn new(fund_name: &str) -> UnclassifiedAssetError {
        UnclassifiedAssetError {
            fund_name: fund_name.to_string(),
        }
    }
}

impl fmt::Display for UnclassifiedAssetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "asset {:} not classified", self.fund_name)
    }
}

impl Error for UnclassifiedAssetError {
    fn description(&self) -> &str {
        "asset not classified"
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub symbol: Option<String>,
    pub value: Decimal,
    pub asset_class: AssetClass,
    // Not strictly necessariy, but helpful for displaying info about the asset
    quantity: Option<Decimal>,
    last_price: Option<Decimal>,
    price_obtained: Option<DateTime<Local>>,
}

impl Asset {
    pub fn new(
        name: String,
        symbol: Option<String>,
        value: Decimal,
        asset_class: AssetClass,
        quantity: Option<Decimal>,
        last_price: Option<Decimal>,
        price_obtained: Option<DateTime<Local>>,
    ) -> Asset {
        Asset {
            name,
            symbol,
            value,
            asset_class,
            quantity,
            last_price,
            price_obtained,
        }
    }
}

impl Asset {
    fn price_is_dated(&self) -> bool {
        match self.price_obtained {
            Some(then) => (Local::now() - then).num_weeks() > 1,
            None => false,
        }
    }
}

impl Ord for Asset {
    /// Sort by ticker name, then by descending value
    fn cmp(&self, other: &Asset) -> Ordering {
        match self.name.cmp(&other.name) {
            Ordering::Equal => other.value.cmp(&self.value),
            less_or_greater => less_or_greater,
        }
    }
}

impl PartialOrd for Asset {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut descriptor = match (self.quantity, self.last_price) {
            (Some(q), Some(p)) => format!("{:} x ${:.2}", q, p),
            (_, _) => String::from("unknown price & quantity"),
        };
        if self.price_is_dated() {
            let last_known = self.price_obtained.unwrap().naive_local();
            descriptor = format!("{:}, {:}", descriptor, last_known.date());
        }

        let label = match &self.symbol {
            Some(symbol) => format!("{:} ({:})", symbol, self.name),
            None => self.name.clone(),
        };
        write!(f, "{:}: ${:.2} ({:})", label, self.value, descriptor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    USBonds,
    USStocks,
    IntlBonds,
    IntlStocks,
    REIT,
    Target,
    Cash,
}

impl fmt::Display for AssetClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            AssetClass::USBonds => "US bonds",
            AssetClass::USStocks => "US stocks",
            AssetClass::IntlBonds => "International bonds",
            AssetClass::IntlStocks => "International stocks",
            AssetClass::REIT => "REIT",
            AssetClass::Target => "Target",
            AssetClass::Cash => "Cash",
        };
        write!(f, "{:}", name)
    }
}

/// This struct is used in 'data/classified.csv' to map from ticker names to asset classes
#[derive(Debug, Deserialize, Serialize)]
struct AssetClassMapping {
    ticker_name: String,
    asset_class: AssetClass,
}

pub struct AssetClassifications {
    mapping: HashMap<String, AssetClass>,
}

impl AssetClassifications {
    pub fn new() -> AssetClassifications {
        AssetClassifications {
            mapping: HashMap::new(),
        }
    }

    fn add(&mut self, name: String, asset_class: AssetClass) {
        self.mapping.insert(name, asset_class);
    }

    pub fn from_csv(path: &str) -> Result<AssetClassifications, Box<dyn Error>> {
        let rdr = csv::Reader::from_path(path)?;
        AssetClassifications::from_reader(rdr)
    }

    fn from_reader<R: io::Read>(
        mut rdr: csv::Reader<R>,
    ) -> Result<AssetClassifications, Box<dyn Error>> {
        let mut asset_classifications = AssetClassifications::new();
        for result in rdr.deserialize() {
            let asset_class: AssetClassMapping = result?;
            asset_classifications.add(asset_class.ticker_name, asset_class.asset_class);
        }
        Ok(asset_classifications)
    }

    pub fn classify(&self, fund_name: &str) -> Result<&AssetClass, UnclassifiedAssetError> {
        self.mapping
            .get(fund_name)
            .ok_or_else(|| UnclassifiedAssetError::new(fund_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_from_empty_csv() {
        let data = "ticker_name,asset_class";
        let rdr = csv::Reader::from_reader(data.as_bytes());
        let ac = AssetClassifications::from_reader(rdr).unwrap();
        assert_eq!(
            ac.classify("VTSAX"),
            Err(UnclassifiedAssetError {
                fund_name: String::from("VTSAX")
            })
        );
    }

    #[test]
    fn test_serializing_from_csv() {
        let data = "ticker_name,asset_class\nVTSAX,USStocks\nVFIAX,USStocks";
        let rdr = csv::Reader::from_reader(data.as_bytes());
        let ac = AssetClassifications::from_reader(rdr).unwrap();
        assert_eq!(
            ac.classify("VTSAX").unwrap().to_owned(),
            AssetClass::USStocks
        );
        assert_eq!(
            ac.classify("VFIAX").unwrap().to_owned(),
            AssetClass::USStocks
        );
        assert_eq!(
            ac.classify("ABCDE"),
            Err(UnclassifiedAssetError {
                fund_name: String::from("ABCDE")
            })
        );
    }

    /// If this fails, it is likely because one of the asset class names was changed!
    #[test]
    fn test_all_asset_classes() {
        let data = "\
ticker_name,asset_class
AAAAA,USBonds
BBBBB,USStocks
CCCCC,IntlBonds
DDDDD,IntlStocks
EEEEE,REIT
FFFFF,Target
GGGGG,Cash";
        let rdr = csv::Reader::from_reader(data.as_bytes());
        AssetClassifications::from_reader(rdr).expect("All asset types are parseable");
    }

    #[test]
    fn included_file_can_be_parsed() {
        AssetClassifications::from_csv("data/classified.csv").expect("File can be parsed!");
    }

    #[test]
    fn asset_with_unknown_ticker() {
        let asset = Asset::new(
            String::from("Private Company"),
            None,
            5196.into(),
            AssetClass::USStocks,
            Some(Decimal::from(400)),
            Some(Decimal::new(1299, 2)),
            None,
        );
        assert_eq!(
            format!("{}", asset),
            "Private Company: $5196.00 (400 x $12.99)"
        );
    }

    #[test]
    fn asset_with_unknown_price_and_quantity() {
        let asset = Asset::new(
            String::from("Vanguard Total Intl Bd Idx Admiral"),
            Some(String::from("VTABX")),
            1234.into(),
            AssetClass::IntlBonds,
            None,
            None,
            None,
        );
        assert_eq!(
            format!("{}", asset),
            "VTABX (Vanguard Total Intl Bd Idx Admiral): $1234.00 (unknown price & quantity)"
        );
    }

    #[test]
    fn asset_last_known_time_missing() {
        let asset = Asset::new(
            String::from("Vanguard Total Intl Bd Idx Admiral"),
            Some(String::from("VTABX")),
            10392.into(),
            AssetClass::IntlBonds,
            Some(Decimal::from(800)),
            Some(Decimal::new(1299, 2)),
            None,
        );
        assert_eq!(
            format!("{}", asset),
            "VTABX (Vanguard Total Intl Bd Idx Admiral): $10392.00 (800 x $12.99)"
        );
    }
}
