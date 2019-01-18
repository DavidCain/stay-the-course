extern crate rust_decimal;

use self::rust_decimal::Decimal;
use assets::{Asset, AssetClass};

#[derive(Debug)]
pub struct AssetAllocation {
    pub asset_class: AssetClass,
    pub target_ratio: Decimal,
    underlying_assets: Vec<Asset>,
    future_contribution: Decimal,
}

pub fn ideal_allocations() -> Vec<AssetAllocation> {
    // TODO: Calculate based off my age, current date
    vec![
        AssetAllocation::new(AssetClass::USBonds, Decimal::new(82, 3)),
        AssetAllocation::new(AssetClass::USStocks, Decimal::new(459, 3)),
        AssetAllocation::new(AssetClass::InternationalStocks, Decimal::new(367, 3)),
        AssetAllocation::new(AssetClass::REIT, Decimal::new(92, 3)),
    ]
}

impl AssetAllocation {
    fn new(asset_class: AssetClass, target_ratio: Decimal) -> AssetAllocation {
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
            panic!("Asset types do not match!");
        }
        self.underlying_assets.push(asset)
    }

    fn percent_holdings(&self, portfolio_total: Decimal) -> Decimal {
        self.future_value() / portfolio_total
    }

    fn deviation(&self, new_total: Decimal) -> Decimal {
        // Identify the percentage of total holdings that this asset will hold
        // (Assesses current value, pending contributions over the eventual total portfolio value)
        let actual = self.percent_holdings(new_total);
        (actual / self.target_ratio) - Decimal::new(1, 0)
    }
}

pub struct Portfolio {
    allocations: Vec<AssetAllocation>,
}

impl Portfolio {
    pub fn new(allocations: Vec<AssetAllocation>) -> Portfolio {
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

        for asset in self.allocations.iter() {
            let start_ratio: Decimal = asset.current_value() / portfolio_total;
            println!(
                " - {:?}: ${:.2}",
                asset.asset_class,
                asset.future_contribution.abs()
            );
            println!(
                "   {:.2}% -> {:.2}% (target: {:.2}%)",
                start_ratio * Decimal::new(100, 0),
                asset.percent_holdings(new_total) * Decimal::new(100, 0),
                asset.target_ratio * Decimal::new(100, 0),
            );
        }
    }
}

pub fn optimally_allocate(mut portfolio: Portfolio, contribution: Decimal) -> Portfolio {
    if contribution == 0.into() {
        panic!("Must deposit or withdraw in order to rebalance");
    }

    if portfolio.sum_target_ratios() != 1.into() {
        panic!("Cannot rebalance unless total is 100%");
    }

    // The amount left for contribution begins as the total amount we have available
    // (We will portion this money out sequentially to each fund, eventually exhausting it)
    let mut amount_left_to_contribute = contribution.clone();

    // The new total is our portfolio's current value, plus the amount we'll contribute
    // In other words, this will be the denomenator for calculating final percent allocation
    let new_total = portfolio.current_value() + contribution;

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

#[test]
fn test_adds_to_1() {
    let terrible_allocation = AssetAllocation::new(AssetClass::Cash, 1.into());
    let portfolio = Portfolio::new(vec![terrible_allocation]);
    optimally_allocate(portfolio, 1_000.into());
}

#[test]
#[should_panic(expected = "Cannot rebalance unless total is 100%")]
fn test_ratios_do_not_sum() {
    let does_not_sum = vec![
        AssetAllocation::new(AssetClass::USStocks, Decimal::new(3, 1)),
        AssetAllocation::new(AssetClass::USBonds, Decimal::new(3, 1)),
    ];
    let portfolio = Portfolio::new(does_not_sum);

    optimally_allocate(portfolio, 1_000.into());
}
