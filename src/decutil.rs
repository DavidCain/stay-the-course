use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub enum InvalidRatioError {
    InvalidDecimal(rust_decimal::Error),
    IncompleteRatio(IncompleteRatioError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteRatioError {
    fraction: String,
}

/**
 * Format the quantity as USD in US locale.
 *
 * Yes, there are better ways to handle locales, but I don't care.
 */
pub fn format_dollars(quantity: &Decimal) -> String {
    let formatted = match quantity.round().to_u64() {
        // If I wanted, could use the `thousands` crate.
        // Some(dollars) => dollars.separate_with_commas()
        Some(dollars) => dollars
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(std::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()
            .unwrap()
            .join(","),
        None => format!("{:.0}", quantity),
    };
    format!("${:}", formatted)
}

impl IncompleteRatioError {
    fn new(fraction: &str) -> IncompleteRatioError {
        IncompleteRatioError {
            fraction: fraction.to_string(),
        }
    }
}

impl fmt::Display for IncompleteRatioError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Cannot parse {:} to a decimal quantity", self.fraction)
    }
}

impl From<rust_decimal::Error> for InvalidRatioError {
    fn from(e: rust_decimal::Error) -> Self {
        InvalidRatioError::InvalidDecimal(e)
    }
}
impl From<IncompleteRatioError> for InvalidRatioError {
    fn from(e: IncompleteRatioError) -> Self {
        InvalidRatioError::IncompleteRatio(e)
    }
}

pub fn frac_to_quantity(fraction: &str) -> Result<Decimal, InvalidRatioError> {
    let mut components = fraction.split('/');
    let numerator = components
        .next()
        .ok_or_else(|| IncompleteRatioError::new(fraction))?;
    let denominator = components
        .next()
        .ok_or_else(|| IncompleteRatioError::new(fraction))?;

    let dec_numerator = Decimal::from_str(numerator)?;
    let dec_denominator = Decimal::from_str(denominator)?;
    Ok(dec_numerator / dec_denominator)
}

pub fn price_to_cents(quantity: &Decimal) -> Option<u64> {
    let rounded_to_whole_cents = (quantity * Decimal::from(100)).round();
    rounded_to_whole_cents.to_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_less_than_one_thousand() {
        assert_eq!(format_dollars(&Decimal::from(150)), "$150");
    }

    #[test]
    fn test_thousands() {
        assert_eq!(format_dollars(&Decimal::from(25123)), "$25,123");
    }

    #[test]
    fn test_millions() {
        assert_eq!(format_dollars(&Decimal::from(9_123_955)), "$9,123,955");
    }

    #[test]
    fn test_rounds_up() {
        assert_eq!(format_dollars(&Decimal::new(123_95593, 2)), "$123,956");
    }

    #[test]
    fn test_rounds_down() {
        assert_eq!(format_dollars(&Decimal::new(123_95547, 2)), "$123,955");
    }

    #[test]
    fn test_incomplete_ratios() {
        fn assert_raises_err(fraction: &str) {
            let error = frac_to_quantity(fraction).err().unwrap();

            match error {
                InvalidRatioError::IncompleteRatio(_) => (),
                _ => panic!("Unexpected error!"),
            }
        }
        assert_raises_err("1");
    }

    #[test]
    fn test_bad_fractions() {
        fn assert_raises_err(fraction: &str) {
            let error = frac_to_quantity(fraction).err().unwrap();

            match error {
                InvalidRatioError::InvalidDecimal(_) => (),
                _ => panic!("Unexpected error!"),
            }
        }
        assert_raises_err("/2");
        assert_raises_err("1/");
    }

    #[test]
    fn test_frac_to_quantity() {
        assert_eq!(frac_to_quantity("1/2").unwrap(), Decimal::new(50, 2));
        assert_eq!(frac_to_quantity("3/4").unwrap(), Decimal::new(75, 2));
        assert_eq!(frac_to_quantity("0/213481143").unwrap(), Decimal::new(0, 0));
    }

    #[test]
    #[should_panic(expected = "Division by zero")]
    fn test_divide_by_zero() {
        frac_to_quantity("1/0").unwrap();
    }

    #[test]
    fn test_price_to_cents() {
        assert_eq!(price_to_cents(&Decimal::new(3525, 2)), Some(3525));
        assert_eq!(
            price_to_cents(&Decimal::from_str("25.4").unwrap()),
            Some(2540)
        );
        assert_eq!(
            price_to_cents(&Decimal::from_str("25").unwrap()),
            Some(2500)
        );
    }

    #[test]
    fn test_negative_prices() {
        assert_eq!(price_to_cents(&Decimal::new(-1, 0)), None);
    }

    #[test]
    fn test_zero_prices() {
        assert_eq!(price_to_cents(&Decimal::new(0, 0)), Some(0));
        assert_eq!(price_to_cents(&Decimal::new(0, 2)), Some(0));
    }

    #[test]
    fn test_fractional_cents() {
        // Fractional cents are rounded away
        assert_eq!(price_to_cents(&Decimal::new(35250, 3)), Some(3525));
        assert_eq!(price_to_cents(&Decimal::new(352505, 4)), Some(3525));

        // Banker's rounding is used!
        assert_eq!(price_to_cents(&Decimal::new(35245, 3)), Some(3524));
        assert_eq!(price_to_cents(&Decimal::new(35255, 3)), Some(3526));
    }
}
