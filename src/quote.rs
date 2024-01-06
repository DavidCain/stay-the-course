use chrono::{DateTime, Local};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};
use std::env;

use crate::dateutil;
use crate::gnucash::Commodity;

use std::fmt;

#[derive(Debug)]
pub struct FinanceQuoteError {
    pub symbol: String,
}

impl fmt::Display for FinanceQuoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to fetch quote")
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub struct JsonQuote {
    #[serde(rename = "01. symbol")]
    pub symbol: String,

    #[serde(
        rename = "07. latest trading day",
        deserialize_with = "simple_noon_datetime"
    )]
    pub time: DateTime<Local>,

    #[serde(rename = "05. price")]
    pub last: Decimal,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub struct GlobalJsonQuote {
    #[serde(rename = "Global Quote")]
    pub quote: JsonQuote,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct Quote {
    pub symbol: String,
    pub time: DateTime<Local>,
    pub last: Decimal,

    pub currency: String,
}

// The AlphaVantage-reported quote "datetime" is a naive date, e.g. 2022-12-25
// We apply the behavior used in the FinanceQuote module -- naively saying it's at noon.
// This satisfies a GnuCash requirement for storing an actual wall time in the db.
fn simple_noon_datetime<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
where
    D: Deserializer<'de>,
{
    let ymd: String = Deserialize::deserialize(deserializer)?;
    // Probably shouldn't assume that the given YMD is valid, but... :shrug:
    Ok(dateutil::localize_at_noon(&ymd).unwrap())
}

pub struct FinanceQuote {}

impl FinanceQuote {
    pub fn fetch_quote(commodity: &Commodity) -> Result<Quote, FinanceQuoteError> {
        let api_key: String = env::var("ALPHAVANTAGE_API_KEY").unwrap();

        let url: String = format!(
            "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={:}&apikey={:}",
            commodity.id, api_key,
        );
        let body = reqwest::blocking::get(url).unwrap().text().unwrap();
        let json_quote: GlobalJsonQuote = serde_json::from_str(&body).unwrap();

        Ok(Quote {
            symbol: json_quote.quote.symbol,
            time: json_quote.quote.time,
            last: json_quote.quote.last,
            currency: String::from("USD"),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Person {
    name: String,
    age: u8,
}

#[cfg(test)]

mod tests {
    use super::*;
    use chrono::{offset::TimeZone, NaiveDateTime};

    #[test]
    fn test_parse_response() {
        let data = r#"{
            "Global Quote": {
                "01. symbol": "FTIAX",
                "02. open": "8.3900",
                "03. high": "8.3900",
                "04. low": "8.3900",
                "05. price": "8.3900",
                "06. volume": "0",
                "07. latest trading day": "2023-12-28",
                "08. previous close": "8.4000",
                "09. change": "-0.0100",
                "10. change percent": "-0.1190%"
            }
        }"#;
        let parsed: GlobalJsonQuote = serde_json::from_str(data).unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2023-12-28T12:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let local: DateTime<Local> = Local.from_local_datetime(&naive).unwrap();

        assert_eq!(
            parsed,
            GlobalJsonQuote {
                quote: JsonQuote {
                    symbol: "FTIAX".into(),
                    last: Decimal::new(83900, 4),
                    time: local,
                }
            }
        )
    }
}
