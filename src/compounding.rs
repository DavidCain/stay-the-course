use chrono::{Local, NaiveDate};
use num::ToPrimitive;
use rust_decimal::Decimal;

fn years_until(future_date: NaiveDate) -> f64 {
    let today: NaiveDate = Local::now().date_naive();
    banking_years(today, future_date)
}

/// Return the banking years between two dates
///
/// APY is usually paid on the full calendar year:
/// Years with 365 days pay the same annual interest as years with 366 (leap years)
fn banking_years(earlier_date: NaiveDate, later_date: NaiveDate) -> f64 {
    assert!(earlier_date < later_date, "Dates must be in order");

    let full_days = (later_date - earlier_date).num_days();

    // TODO: Don't approximate, but actually handle leap years
    (full_days as f64) / 365.25
}

/// Compound the principal, with a given APY, from now until the end date
pub fn compound(principal: Decimal, apy: f64, end_date: NaiveDate) -> Decimal {
    let multiplier = (apy + 1.0).powf(years_until(end_date));
    let dollars = principal.to_f64().unwrap() * multiplier; // Fractional dollars
    let cents = (dollars * 100.0) as i64;
    Decimal::new(cents, 2)
}

/// Identify an annual income that can be safely maintained in perpetuity
pub fn safe_withdrawal_income(principal: Decimal) -> Decimal {
    let safe_withdrawal_rate = Decimal::new(4, 2);
    principal * safe_withdrawal_rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banking_years() {
        let current_date = NaiveDate::from_ymd(2019, 4, 18);
        let future_date = NaiveDate::from_ymd(2095, 4, 18);
        assert_eq!(banking_years(current_date, future_date), 76.0);
    }

    #[test]
    fn test_compounding() {
        let future_date = NaiveDate::from_ymd(2055, 4, 18);
        let total = compound(Decimal::from(100_000), 0.07, future_date);
        assert!(total > Decimal::from(100_000));
        // TODO: This value is hard-coded from today's date (July 9, 2019)
        // To properly test, we need to mock current moment.
        //assert_eq!(total, Decimal::new(112517280, 2));
    }

    #[test]
    fn test_swr() {
        assert_eq!(safe_withdrawal_income(1_000_000.into()), 40_000.into());
        assert_eq!(safe_withdrawal_income(2_000_000.into()), 80_000.into());
        assert_eq!(safe_withdrawal_income(3_000_000.into()), 120_000.into());
    }
}
