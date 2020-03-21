extern crate chrono;
extern crate lexpr;
extern crate rust_decimal;
extern crate serde;
extern crate serde_lexpr;

use self::chrono::{DateTime, Local};
use self::rust_decimal::Decimal;
use self::serde::{de, Deserialize, Deserializer};
use std::io::Read;
use std::io::Write;
use std::process::{Command, Stdio};

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
pub struct Quote {
    pub symbol: String,
    #[serde(rename = "gnc:time-no-zone", deserialize_with = "simple_datetime")]
    pub time: DateTime<Local>,
    pub last: Decimal,
    pub currency: String,
}

// The given datetime is formatted as `2019-12-11 12:00:00`
// By default, serde works with RFC3339 format.
fn simple_datetime<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    dateutil::localize_naive_dt(&s).map_err(de::Error::custom)
}

pub struct FinanceQuote {}

impl FinanceQuote {
    pub fn fetch_quote(commodity: &Commodity) -> Result<Quote, FinanceQuoteError> {
        // echo '(alphavantage "VTSAX")' | /Applications/Gnucash.app/Contents/Resources/bin/gnc-fq-helper
        let process =
            // TODO: Specify from config
            Command::new("/Applications/Gnucash.app/Contents/Resources/bin/gnc-fq-helper")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("failed to execute Finance::Quote");

        let commodity_desc = format!("(alphavantage \"{:}\")", commodity.id);
        process
            .stdin
            .unwrap()
            .write_all(commodity_desc.as_bytes())
            .unwrap();

        let mut s = String::new();
        process.stdout.unwrap().read_to_string(&mut s).unwrap();

        match FinanceQuote::parse_quote_l_expr(&s) {
            Some(quote) => Ok(quote),
            None => Err(FinanceQuoteError {
                symbol: commodity.id.clone(),
            }),
        }
    }

    /**
     * Parse the S-expression that results a quote into a Quote struct.
     */
    fn parse_quote_l_expr(data: &str) -> Option<Quote> {
        if data.trim() == "(#f)" {
            return None;
        }

        // Left side of the tuple is the symbol, right side is the result!
        // Because left side (symbol) is juts a string, we cannot easily represent the whole
        // expression as a deserializable struct.
        //
        // We toss out the symbol on the left side of the tuple, then deserialize the remainder.
        let v = lexpr::from_str(data).unwrap_or_else(|_| panic!("Invalid s-expression {:}", data));
        let symbol_and_quote = v[0]
            .as_pair()
            .unwrap_or_else(|| panic!("Expected tuple {:}", data));
        let quote = symbol_and_quote.1;

        Some(serde_lexpr::from_str(&quote.to_string()).unwrap())
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

    #[test]
    fn test_parse_quote() {
        // Test sample return value! Raw result (no spaces) is:
        // (("VBTLX" (symbol . "VBTLX") (gnc:time-no-zone . "2019-12-11 12:00:00") (last . 11.0900) (currency . "USD")))
        let data = r#"(
                       ("VBTLX" (symbol . "VBTLX")
                                (gnc:time-no-zone . "2019-12-11 12:00:00")
                                (last . 11.0900)
                                (currency . "USD")
                       )
                      )"#;
        let quote_from_data = FinanceQuote::parse_quote_l_expr(data).unwrap();
        assert_eq!(
            quote_from_data,
            Quote {
                symbol: "VBTLX".into(),
                last: Decimal::new(1109, 2),
                time: quote_from_data.time,
                currency: "USD".into()
            }
        )
    }

    #[test]
    fn test_failure() {
        let result = FinanceQuote::parse_quote_l_expr("(#f)");
        assert!(result.is_none());

        let trailing_whitespace_result = FinanceQuote::parse_quote_l_expr("(#f)\n");
        assert!(trailing_whitespace_result.is_none());
    }
}
