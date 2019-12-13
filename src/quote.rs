extern crate chrono;
extern crate lexpr;
extern crate rust_decimal;
extern crate serde;
extern crate serde_lexpr;

use self::chrono::NaiveDateTime;
use self::rust_decimal::Decimal;
use self::serde::{de, Deserialize, Deserializer};
use std::io::Read;
use std::io::Write;
use std::process::{Command, Stdio};

use gnucash::Commodity;

#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd)]
pub struct Quote {
    pub symbol: String,
    #[serde(rename = "gnc:time-no-zone", deserialize_with = "simple_datetime")]
    pub time: NaiveDateTime,
    pub last: Decimal,
    pub currency: String,
}

// The given datetime is formatted as `2019-12-11 12:00:00`
// By default, serde works with RFC3339 format.
fn simple_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").map_err(de::Error::custom)
}

pub struct FinanceQuote {}

impl FinanceQuote {
    pub fn fetch_quote(commodity: &Commodity) -> Quote {
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

        FinanceQuote::parse_quote_l_expr(&s)
    }

    /**
     * Parse the S-expression that results a quote into a Quote struct.
     */
    fn parse_quote_l_expr(data: &str) -> Quote {
        // Left side of the tuple is the symbol, right side is the result!
        // Because left side (symbol) is juts a string, we cannot easily represent the whole
        // expression as a deserializable struct.
        //
        // We toss out the symbol on the left side of the tuple, then deserialize the remainder.
        let v = lexpr::from_str(data).unwrap();
        let symbol_and_quote = v[0].as_pair().unwrap();
        let quote = symbol_and_quote.1;

        serde_lexpr::from_str(&quote.to_string()).unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Person {
    name: String,
    age: u8,
}

#[cfg(test)]
mod tests {
    use self::chrono::NaiveDate;
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
        assert_eq!(
            FinanceQuote::parse_quote_l_expr(data),
            Quote {
                symbol: "VBTLX".into(),
                last: Decimal::new(1109, 2),
                time: NaiveDate::from_ymd(2019, 12, 11).and_hms(12, 0, 0),
                currency: "USD".into()
            }
        )
    }
}
