extern crate decimal;
extern crate ord_subset;

use self::decimal::d128;
use self::ord_subset::OrdVar;
use assets::{Asset, AssetClass};

#[derive(Debug)]
pub struct AssetAllocation {
    pub asset_class: AssetClass,
    pub target_ratio: d128,
    underlying_assets: Vec<Asset>,
    future_contribution: d128,
}

pub fn ideal_allocations() -> Vec<AssetAllocation> {
    // TODO: Calculate based off my age, current date
    vec![
        AssetAllocation::new(AssetClass::USBonds, d128!(0.082)),
        AssetAllocation::new(AssetClass::USStocks, d128!(0.459)),
        AssetAllocation::new(AssetClass::InternationalStocks, d128!(0.367)),
        AssetAllocation::new(AssetClass::REIT, d128!(0.092)),
    ]
}

impl AssetAllocation {
    fn new(asset_class: AssetClass, target_ratio: d128) -> AssetAllocation {
        let underlying_assets = Vec::new();
        let future_contribution = d128::zero();

        AssetAllocation {
            asset_class,
            underlying_assets,
            target_ratio,
            future_contribution,
        }
    }

    pub fn add_contribution(&mut self, contribution: d128) {
        self.future_contribution += contribution;
    }

    fn current_value(&self) -> d128 {
        self.underlying_assets
            .iter()
            .fold(d128::zero(), |total, asset| total + asset.value)
    }

    fn future_value(&self) -> d128 {
        self.current_value() + self.future_contribution
    }

    pub fn add_asset(&mut self, asset: Asset) {
        if asset.asset_class != self.asset_class {
            panic!("Asset types do not match!");
        }
        self.underlying_assets.push(asset)
    }

    fn percent_holdings(&self, portfolio_total: d128) -> d128 {
        self.future_value() / portfolio_total
    }

    fn deviation(&self, new_total: d128) -> d128 {
        // Identify the percentage of total holdings that this asset will hold
        // (Assesses current value, pending contributions over the eventual total portfolio value)
        let actual = self.percent_holdings(new_total);
        (actual / self.target_ratio) - d128!(1)
    }
}

pub struct Portfolio {
    allocations: Vec<AssetAllocation>,
}

impl Portfolio {
    pub fn new(allocations: Vec<AssetAllocation>) -> Portfolio {
        Portfolio { allocations }
    }

    fn current_value(&self) -> d128 {
        self.allocations
            .iter()
            .fold(d128::zero(), |total, allocation| {
                total + &allocation.current_value()
            })
    }

    fn future_value(&self) -> d128 {
        self.allocations
            .iter()
            .fold(d128::zero(), |total, allocation| {
                total + &allocation.future_value()
            })
    }

    fn sum_target_ratios(&self) -> d128 {
        self.allocations
            .iter()
            .fold(d128::zero(), |total, allocation| {
                total + &allocation.target_ratio
            })
    }

    fn num_asset_classes(&self) -> usize {
        self.allocations.len()
    }

    pub fn describe_future_contributions(&self) {
        let portfolio_total = self.current_value();
        let new_total = self.future_value();

        println!(
            "Portfolio value before: {:?} after: {:?}",
            portfolio_total, new_total
        );
        for asset in self.allocations.iter() {
            let start_ratio = asset.current_value() / portfolio_total;
            println!(
                "Contribute ${:?} to {:?}",
                asset.future_contribution, asset.asset_class
            );
            println!(
                "    Target: {:?} Start: {:?}% Final: {:?}%",
                asset.target_ratio * d128!(100),
                start_ratio * d128!(100),
                asset.percent_holdings(new_total) * d128!(100)
            );
        }
    }
}

pub fn optimally_allocate(mut portfolio: Portfolio, contribution: d128) -> Portfolio {
    if contribution.is_zero() {
        panic!("Must deposit or withdraw in order to rebalance");
    }

    if portfolio.sum_target_ratios() != d128!(1) {
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
    portfolio.allocations.sort_by(|a, b| {
        OrdVar::new(a.deviation(new_total)).cmp(&OrdVar::new(b.deviation(new_total)))
    });
    if contribution.is_negative() {
        portfolio.allocations.reverse();
    }

    let num_assets = portfolio.num_asset_classes();

    let (deviation_target, index_to_stop): (d128, usize) = {
        // As we loop through assets, we track the sum of all ideal fund values
        let mut summed_targets_of_affected_assets = d128::zero();

        // We iterate through assets based on which need alteration first (to minimize variation)
        // We may not end up depositing/withdrawing from all accounts.
        //
        // As we loop through, we keep track of:
        // 1. Which assets receive deposits/withdrawals (first through `last_known_index`)
        // 2. The fractional deviation used to calculate the magnitude of deposits/withdrawals
        //    `deviation_target` will be the deviation (or "approximation error") of the last
        //    asset class we're optimizing.
        let mut deviation_target = d128::zero();
        let mut last_known_index = 0;

        for (index, asset) in portfolio.allocations.iter().enumerate() {
            assert!(amount_left_to_contribute.abs() > d128::zero());

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
                d128::zero()
            } else {
                portfolio.allocations[index + 1].deviation(new_total)
            };

            // Solve for the amount that brings this asset as close to its target as the next closest
            let delta =
                summed_targets_of_affected_assets * (next_lowest_deviation - deviation_target);

            if delta.abs() > amount_left_to_contribute.abs() {
                // If we don't have enough money left to contribute the full amount, then we'll
                // dedicate what's left to the given fund, and exit.
                deviation_target += amount_left_to_contribute / summed_targets_of_affected_assets;
                amount_left_to_contribute = d128::zero();
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
            if amount_left_to_contribute.is_zero() {
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
    let terrible_allocation = AssetAllocation::new(AssetClass::Cash, d128!(1));
    let portfolio = Portfolio::new(vec![terrible_allocation]);
    optimally_allocate(portfolio, d128!(1_000));
}

#[test]
#[should_panic(expected = "Cannot rebalance unless total is 100%")]
fn test_ratios_do_not_sum() {
    let does_not_sum = vec![
        AssetAllocation::new(AssetClass::USStocks, d128!(0.3)),
        AssetAllocation::new(AssetClass::USBonds, d128!(0.3)),
    ];
    let portfolio = Portfolio::new(does_not_sum);

    optimally_allocate(portfolio, d128!(1000));
}
