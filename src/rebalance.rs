extern crate rust_decimal;

use self::rust_decimal::Decimal;
use assets::{Asset, AssetClass};
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct AssetAllocation {
    pub asset_class: AssetClass,
    pub target_ratio: Decimal,
    underlying_assets: Vec<Asset>,
    future_contribution: Decimal,
}

impl Ord for AssetAllocation {
    /// Sort by descending value (largest allocations first)
    /// Ordering only takes _current_ values into consideration
    fn cmp(&self, other: &AssetAllocation) -> Ordering {
        other.current_value().cmp(&self.current_value())
    }
}

impl PartialOrd for AssetAllocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl AssetAllocation {
    pub fn new(asset_class: AssetClass, target_ratio: Decimal) -> AssetAllocation {
        let underlying_assets = Vec::new();
        let future_contribution = 0.into();

        AssetAllocation {
            asset_class,
            underlying_assets,
            target_ratio,
            future_contribution,
        }
    }

    pub fn add_contribution(&mut self, contribution: Decimal) {
        self.future_contribution += contribution;
    }

    fn current_value(&self) -> Decimal {
        self.underlying_assets
            .iter()
            .fold(0.into(), |total, asset| total + asset.value)
    }

    fn future_value(&self) -> Decimal {
        self.current_value() + self.future_contribution
    }

    pub fn add_asset(&mut self, asset: Asset) {
        if asset.asset_class != self.asset_class {
            panic!("Asset types must match");
        }
        self.underlying_assets.push(asset);
        // TODO: Could use a BinaryHeap instead for better efficiency
        self.underlying_assets.sort();
    }

    fn percent_holdings(&self, portfolio_total: Decimal) -> Decimal {
        self.future_value() / portfolio_total
    }

    fn deviation(&self, new_total: Decimal) -> Decimal {
        // Identify the percentage of total holdings that this asset will hold
        // (Assesses current value, pending contributions over the eventual total portfolio value)
        let actual = self.percent_holdings(new_total);
        (actual / self.target_ratio) - Decimal::from(1)
    }
}

impl fmt::Display for AssetAllocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:}: ${:.0} (🎯 {:.2}%)",
            self.asset_class,
            self.current_value(),
            self.target_ratio * Decimal::from(100)
        )?;

        for asset in &self.underlying_assets {
            write!(f, "\n  - {:}", asset)?;
        }
        Ok(())
    }
}

pub struct Portfolio {
    allocations: Vec<AssetAllocation>,
}

impl fmt::Display for Portfolio {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Display allocations in order, starting from the largest
        for allocation in (&self.allocations).into_iter() {
            writeln!(f, "{:}", allocation)?;
        }
        write!(f, "Portfolio total: ${:.0}", self.current_value())
    }
}

impl Portfolio {
    pub fn new(mut allocations: Vec<AssetAllocation>) -> Portfolio {
        allocations.sort();
        Portfolio { allocations }
    }

    pub fn current_value(&self) -> Decimal {
        self.allocations.iter().fold(0.into(), |total, allocation| {
            total + &allocation.current_value()
        })
    }

    fn future_value(&self) -> Decimal {
        self.allocations.iter().fold(0.into(), |total, allocation| {
            total + &allocation.future_value()
        })
    }

    fn sum_target_ratios(&self) -> Decimal {
        self.allocations.iter().fold(0.into(), |total, allocation| {
            total + &allocation.target_ratio
        })
    }

    fn num_asset_classes(&self) -> usize {
        self.allocations.len()
    }

    pub fn describe_future_contributions(&self) {
        let portfolio_total = self.current_value();
        let new_total = self.future_value();
        let verb = if new_total < portfolio_total {
            "Withdraw"
        } else {
            "Contribute"
        };
        println!("{:} the following amounts:", verb);

        let zero: Decimal = 0.into();
        for asset in self.allocations.iter() {
            let start_ratio: Decimal = if portfolio_total == zero {
                // If our starting portfolio was empty, we don't want to divide by zero
                // Treat an asset classs as holding 0% of an empty portfolio
                zero
            } else {
                asset.current_value() / portfolio_total
            };
            println!(
                " - {:}: ${:.2}",
                asset.asset_class,
                asset.future_contribution.abs()
            );
            println!(
                "   {:.2}% -> {:.2}% (target: {:.2}%)",
                start_ratio * Decimal::from(100),
                asset.percent_holdings(new_total) * Decimal::from(100),
                asset.target_ratio * Decimal::from(100),
            );
        }
    }
}

fn proportionally_allocate(mut portfolio: Portfolio, contribution: Decimal) -> Portfolio {
    for mut asset in portfolio.allocations.iter_mut() {
        let amount = asset.target_ratio * contribution;
        asset.add_contribution(amount);
    }
    portfolio
}

pub fn optimally_allocate(mut portfolio: Portfolio, contribution: Decimal) -> Portfolio {
    if contribution == 0.into() {
        panic!("Must deposit or withdraw in order to rebalance");
    }

    if portfolio.sum_target_ratios() != 1.into() {
        panic!("Cannot rebalance unless total is 100%");
    }

    let current_value = portfolio.current_value();
    if contribution.is_sign_negative() {
        assert!(
            contribution.abs() < current_value,
            "Cannot withdraw more than portfolio!"
        );
    }
    if current_value == 0.into() {
        return proportionally_allocate(portfolio, contribution);
    }

    assert!(
        !current_value.is_sign_negative(),
        "Can't handle a portfolio with a negative balance"
    );

    // The amount left for contribution begins as the total amount we have available
    // (We will portion this money out sequentially to each fund, eventually exhausting it)
    let mut amount_left_to_contribute = contribution.clone();

    // The new total is our portfolio's current value, plus the amount we'll contribute
    // In other words, this will be the denomenator for calculating final percent allocation
    let new_total = current_value + contribution;

    // We sort our asset allocations by how much they've deviated from their target
    // If contributing: underallocated funds come first. Overallocated funds come last.
    // If withdrawing: overallocated funds come first. Underallocated funds come last.
    portfolio
        .allocations
        .sort_by(|a, b| a.deviation(new_total).cmp(&b.deviation(new_total)));
    if contribution.is_sign_negative() {
        portfolio.allocations.reverse();
    }

    let num_assets = portfolio.num_asset_classes();

    let (deviation_target, index_to_stop): (Decimal, usize) = {
        // As we loop through assets, we track the sum of all ideal fund values
        let mut summed_targets_of_affected_assets: Decimal = 0.into();

        // We iterate through assets based on which need alteration first (to minimize variation)
        // We may not end up depositing/withdrawing from all accounts.
        //
        // As we loop through, we keep track of:
        // 1. Which assets receive deposits/withdrawals (first through `last_known_index`)
        // 2. The fractional deviation used to calculate the magnitude of deposits/withdrawals
        //    `deviation_target` will be the deviation (or "approximation error") of the last
        //    asset class we're optimizing.
        let mut deviation_target = 0.into();
        let mut last_known_index = 0;

        for (index, asset) in portfolio.allocations.iter().enumerate() {
            assert!(amount_left_to_contribute.abs() > 0.into());

            // Because we have money left to distribute, we know the asset at portfolio.allocations[index] will
            // be affected (receiving deposits if amount > 0, or withdrawn if amount < 0)
            last_known_index = index;

            // Identify how much this asset's allocation deviates from its target
            // On the last loop iteration, this target is used to calculate final asset deltas
            deviation_target = asset.deviation(new_total);

            // Identify the total value of this asset that brings it in line with our target ratio
            // Importantly, this is the total value _with the new contribution included_
            // (We can use this value to calculate required deposits/withdrawals)
            let target_value = new_total * asset.target_ratio;

            summed_targets_of_affected_assets += target_value;

            // Peek ahead in the vector to get the asset which is the second-most underallocated
            // (We will contribute proportionally until all assets are at least that close to their target)
            let next_lowest_deviation = if index >= (num_assets - 1) {
                0.into()
            } else {
                portfolio.allocations[index + 1].deviation(new_total)
            };

            // Solve for the amount that brings this asset as close to its target as the next closest
            let delta: Decimal =
                summed_targets_of_affected_assets * (next_lowest_deviation - deviation_target);

            if delta.abs() > amount_left_to_contribute.abs() {
                // If we don't have enough money left to contribute the full amount, then we'll
                // dedicate what's left to the given fund, and exit.
                deviation_target += amount_left_to_contribute / summed_targets_of_affected_assets;
                amount_left_to_contribute = 0.into();
            } else {
                // Otherwise, this asset is now as close to its target as the next worst asset(s)
                // We continue by bringing these assets closer to their targets
                amount_left_to_contribute -= delta;
                deviation_target = next_lowest_deviation;
            }

            // Two cases bring us to an exit:
            // 1. We contributed the exact amount to bring the asset as close to its target as the
            //    next worst (rare, but possible)
            // 2. We were not able to contribute the full amount, so we contributed what was left
            if amount_left_to_contribute == 0.into() {
                break;
            }
        }

        let index_to_stop = last_known_index + 1;
        (deviation_target, index_to_stop)
    };

    for (index, asset) in portfolio.allocations.iter_mut().enumerate() {
        if index == index_to_stop {
            break;
        }
        let target_value = new_total * asset.target_ratio;
        let deviation = asset.deviation(new_total);

        let delta = target_value * (deviation_target - deviation);

        asset.add_contribution(delta);
    }

    portfolio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "Asset types must match")]
    fn test_asset_types_must_match() {
        let mut stocks = AssetAllocation::new(AssetClass::USStocks, 1.into());

        stocks.add_asset(Asset::new(
            String::from("VTABX"),
            1234.into(),
            AssetClass::IntlBonds,
            None,
            None,
            None,
        ));
    }

    #[test]
    fn test_current_value_is_summed_assets() {
        let mut stocks = AssetAllocation::new(AssetClass::USStocks, 1.into());
        assert_eq!(stocks.current_value(), 0.into());

        stocks.add_asset(Asset::new(
            String::from("VTSAX"),
            8675.into(),
            AssetClass::USStocks,
            None,
            None,
            None,
        ));

        assert_eq!(stocks.current_value(), Decimal::from(8675));

        stocks.add_asset(Asset::new(
            String::from("FZROX"),
            10000.into(),
            AssetClass::USStocks,
            None,
            None,
            None,
        ));

        assert_eq!(stocks.current_value(), Decimal::from(18675));
    }
    #[test]
    fn test_add_contribution() {
        let mut bonds = AssetAllocation::new(AssetClass::USBonds, 1.into());

        // Object starts with no known assets, no future contribution
        assert_eq!(bonds.current_value(), 0.into());
        assert_eq!(bonds.future_value(), 0.into());

        // We add $37.20 as a future contribution
        bonds.add_contribution(Decimal::new(3720, 2));
        assert_eq!(bonds.current_value(), 0.into());
        assert_eq!(bonds.future_value(), Decimal::new(3720, 2));

        // We add another future contribution ($14.67)
        bonds.add_contribution(Decimal::new(1467, 2));
        assert_eq!(bonds.current_value(), 0.into());
        assert_eq!(bonds.future_value(), Decimal::new(5187, 2));
    }

    #[test]
    fn test_allocations_sum_to_1() {
        let terrible_allocation = AssetAllocation::new(AssetClass::Cash, 1.into());
        let portfolio = Portfolio::new(vec![terrible_allocation]);
        optimally_allocate(portfolio, 1_000.into());
    }

    #[test]
    #[should_panic(expected = "Cannot rebalance unless total is 100%")]
    fn test_allocations_do_not_sum() {
        let does_not_sum = vec![
            AssetAllocation::new(AssetClass::USStocks, Decimal::new(3, 1)),
            AssetAllocation::new(AssetClass::USBonds, Decimal::new(3, 1)),
        ];
        let portfolio = Portfolio::new(does_not_sum);

        optimally_allocate(portfolio, 1_000.into());
    }

    #[test]
    fn test_should_sort_by_current_allocation_value() {
        let mut stocks = AssetAllocation::new(AssetClass::USStocks, Decimal::new(50, 2));
        let mut bonds = AssetAllocation::new(AssetClass::USBonds, Decimal::new(50, 2));

        // We keep $10 in bonds, but plan to contribute nearly $1 million in stocks
        bonds.add_asset(Asset::new(
            String::from("VBTLX"),
            10.into(),
            AssetClass::USBonds,
            None,
            None,
            None,
        ));
        stocks.add_contribution(999_999.into());

        // Ordering is done by current value.
        let mut allocations = vec![&stocks, &bonds];
        allocations.sort();
        assert_eq!(allocations, vec![&bonds, &stocks]);
    }
}
