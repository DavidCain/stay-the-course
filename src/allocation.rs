use crate::assets::AssetClass;
use crate::rebalance::AssetAllocation;
use chrono::{Local, NaiveDate};
use rust_decimal::Decimal;

fn age_in_weeks(birthday: NaiveDate) -> i64 {
    let today: NaiveDate = Local::now().date_naive();
    assert!(birthday < today, "You were born in the future?");
    (today - birthday).num_weeks()
}

/// Derive bond allocation from the "your age in bonds" principle.
///
/// "Own your age" in bonds is a simple concept: If you're 45, then 45% of your holdings should be
/// in bonds. This rule ensures that risk preference gradually becomes more conservative as you
/// approach retirement.
///
/// However, some people consider this strategy too conservative. By revising the rule to be "110
/// minus your age in stocks" (with the rest in bonds) a more risk-prone strategy arises. "120
/// minus your age in stocks" is even more risk-loving.
///
/// One's age can be expressed as a whole integer (e.g. 45 years old), or as a ratio of arbitrary
/// precision (45.1705... or 45 years, 2 months, 3 days, 8 hours, 2 minutes...). Changing your
/// optimal bond allocation every minute is somewhat ridiculous, but making periodic adjustments
/// through the year ensures a gradual transition (rather than a one-point jump on your birthday).
/// This function strikes a compromise, and gives allocations rounded to the week.
///
pub fn bond_allocation(birthday: NaiveDate, from_years: u8) -> Decimal {
    let age = Decimal::from(age_in_weeks(birthday)) / Decimal::from(52);

    let mut stock_allocation = Decimal::from(from_years) - age;
    stock_allocation = stock_allocation.round_dp(2);
    let scale = &stock_allocation.scale();
    stock_allocation.set_scale(scale + 2).unwrap(); // Convert to an actual ratio

    // Young investors could end up with a _negative_ bond allocation
    // (Very old investors could end up with a bond allocation > 100%!)
    // Neither situation makes sense. Make sure we stay within 0 -> 100%
    if stock_allocation.is_sign_negative() {
        return Decimal::from(1);
    } else if stock_allocation > Decimal::from(1) {
        return Decimal::from(0);
    }
    Decimal::from(1) - stock_allocation
}

/// Return an asset allocation based on Rick Ferri's ["Core Four" Strategy][core-four].
///
/// Given a bond allocation, this strategy splits the remaining funds:
///  - 50% to US Stock
///  - 40% to International Stock
///  - 10% to REIT
///
/// [core-four]: https://www.bogleheads.org/wiki/Lazy_portfolios#Core_four_portfolios
///
/// I add a simple modification, which is to not exceed 50% of my US stocks in large cap.
/// Some of my largest funds are heavily weighted towards large cap:
///  - VTSAX (~72% large cap):
///      - 41%  Giant Cap
///      - 31%  Large Cap
///      - 19%  Mid Cap
///      - 6%   Small Cap
///      - 2%   Micro Cap
///  - VFIAX (~84% large cap)
///      - 50%  Giant Cap
///      - 34%  Largo Cap
///      - 16%  Mid Cap
///  - FZILX (~84% large cap)
///      - 49%  Giant Cap
///      - 36%  Largo Cap
///      - 15%  Mid Cap
/// For simplicity, just say that any US Total Market fund is 75% large cap.
/// Accordingly, for a 50/50 split of small+mid vs. large+giant,
/// I want $1 in VSMAX for every $2 in VTSAX.
///
pub fn core_four(ratio_bonds: Decimal) -> Vec<AssetAllocation> {
    let one: Decimal = 1.into();

    assert!(!ratio_bonds.is_sign_negative(), "Ratio must be positive");
    assert!(ratio_bonds <= one, "Ratio cannot exceed 100%");

    let ratio_stocks: Decimal = one - ratio_bonds;

    vec![
        // Allocate ratio of bonds
        AssetAllocation::new(AssetClass::USBonds, ratio_bonds),
        // Split remaining funds between US Total, US Small/Mid, International, and REIT
        AssetAllocation::new(AssetClass::USTotal, Decimal::new(33, 2) * ratio_stocks),
        AssetAllocation::new(AssetClass::USSmall, Decimal::new(17, 2) * ratio_stocks),
        AssetAllocation::new(AssetClass::IntlStocks, Decimal::new(40, 2) * ratio_stocks),
        AssetAllocation::new(AssetClass::REIT, Decimal::new(10, 2) * ratio_stocks),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "You were born in the future?")]
    fn test_future_birthday() {
        let birthday = NaiveDate::from_ymd(2095, 6, 14);
        age_in_weeks(birthday);
    }

    #[test]
    fn test_bond_allocation_ancient_investor() {
        let birthday = NaiveDate::from_ymd(1863, 11, 19);
        assert_eq!(bond_allocation(birthday, 100), Decimal::from(1));
    }

    #[test]
    fn test_bond_allocation_very_young_investor() {
        let birthday = NaiveDate::from_ymd(2018, 12, 30);
        assert_eq!(bond_allocation(birthday, 130), Decimal::from(0));
    }

    #[test]
    #[should_panic(expected = "Ratio must be positive")]
    fn test_negative_ratio() {
        let negative = Decimal::new(-20, 3);
        core_four(negative);
    }

    #[test]
    #[should_panic(expected = "Ratio cannot exceed 100%")]
    fn test_exceeds_one_hundred_percent() {
        core_four(2.into());
    }

    #[test]
    fn test_core_four_all_stocks() {
        assert_eq!(
            core_four(0.into()),
            vec![
                AssetAllocation::new(AssetClass::USBonds, 0.into()),
                AssetAllocation::new(AssetClass::USTotal, Decimal::new(33, 2)),
                AssetAllocation::new(AssetClass::USSmall, Decimal::new(17, 2)),
                AssetAllocation::new(AssetClass::IntlStocks, Decimal::new(40, 2)),
                AssetAllocation::new(AssetClass::REIT, Decimal::new(10, 2)),
            ]
        );
    }

    #[test]
    fn test_core_four_young() {
        assert_eq!(
            core_four(Decimal::new(20, 2)),
            vec![
                AssetAllocation::new(AssetClass::USBonds, Decimal::new(20, 2)),
                AssetAllocation::new(AssetClass::USTotal, Decimal::new(264, 3)),
                AssetAllocation::new(AssetClass::USSmall, Decimal::new(136, 3)),
                AssetAllocation::new(AssetClass::IntlStocks, Decimal::new(32, 2)),
                AssetAllocation::new(AssetClass::REIT, Decimal::new(8, 2)),
            ]
        );
    }

    #[test]
    fn test_core_four_middle_aged() {
        assert_eq!(
            core_four(Decimal::new(60, 2)),
            vec![
                AssetAllocation::new(AssetClass::USBonds, Decimal::new(60, 2)),
                AssetAllocation::new(AssetClass::USTotal, Decimal::new(132, 3)),
                AssetAllocation::new(AssetClass::USSmall, Decimal::new(68, 3)),
                AssetAllocation::new(AssetClass::IntlStocks, Decimal::new(16, 2)),
                AssetAllocation::new(AssetClass::REIT, Decimal::new(4, 2)),
            ]
        );
    }
}
