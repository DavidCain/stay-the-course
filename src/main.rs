#[macro_use]
extern crate serde_derive;

use chrono::{Datelike, Local, NaiveDate};
use rust_decimal::Decimal;
use std::cmp;
use std::io;

mod allocation;
mod assets;
mod compounding;
mod config;
mod dateutil;
mod decutil;
mod gnucash;
mod quote;
mod rebalance;
mod stats;

use crate::config::Config;
use crate::gnucash::Book;

fn get_contribution() -> Decimal {
    let mut contribution = String::new();

    println!("How much to contribute or withdraw?");
    io::stdin()
        .read_line(&mut contribution)
        .expect("Failed to read line");

    contribution.trim().parse().expect("Please type a number!")
}

fn summarize_retirement_prospects(birthday: NaiveDate, portfolio_total: Decimal, real_apy: f64) {
    println!(
        "Worth at retirement (Assuming {:.0}% growth):",
        real_apy * 100.0
    );

    fn summarize(day_of_retirement: NaiveDate, birthday: NaiveDate, future_total: Decimal) {
        assert!(
            day_of_retirement > birthday,
            "Cannot retire before being born..."
        );
        // TODO: Correctly calculate age instead of this cheap approximation
        let retirement_age = day_of_retirement.year() - birthday.year();
        println!(
            // Neatly displays net worth up to $25MM
            // If your assets are that high, why are you running this jank?
            " - {}: {: >11}  SWR: {: >9}",
            retirement_age,
            decutil::format_dollars(&future_total),
            decutil::format_dollars(&compounding::safe_withdrawal_income(future_total))
        );
    }

    let today = Local::now().date_naive();
    summarize(today, birthday, portfolio_total);

    let approx_age = today.year() - birthday.year(); // Could be this age, or one year younger
    let start_age = cmp::max(50, approx_age + 5);

    let retirement_ages = (start_age)..=(start_age + 15);
    for age in retirement_ages.step_by(5) {
        let year = birthday.year() + age;
        // Subtle bug here -- Feb 29th doesn't exist in some years.
        // Ignore it for now.
        let day_of_retirement =
            NaiveDate::from_ymd_opt(year, birthday.month(), birthday.day()).unwrap();
        let future_total = compounding::compound(portfolio_total, real_apy, day_of_retirement);
        summarize(day_of_retirement, birthday, future_total);
    }
    println!();
}

fn main() {
    let conf = Config::from_file("config.toml");
    let book = Book::from_config(&conf);
    println!("-----------------------------------------------------------------------");

    // Identify our ideal allocations (percentages by asset class, summing to 100%)
    let birthday = conf.user_birthday();
    let bond_allocation = allocation::bond_allocation(birthday, 120);
    let ideal_allocations = allocation::core_four(bond_allocation);

    let asset_classifications =
        assets::AssetClassifications::from_csv("data/classified.csv").unwrap();
    let portfolio = book.portfolio_status(asset_classifications, ideal_allocations);

    println!("{:}\n", portfolio);

    summarize_retirement_prospects(birthday, portfolio.current_value(), 0.07);

    if conf.gnucash.file_format == "sqlite3" {
        let sql_stats = stats::Stats::new(&conf.gnucash.path_to_book);
        let after_tax = sql_stats.after_tax_income().unwrap();
        let charity = sql_stats.charitable_giving().unwrap();
        println!("After-tax income: {:}", decutil::format_dollars(&after_tax));
        println!(
            "Charitable giving: {:} ({:.0}% of after-tax income)",
            decutil::format_dollars(&charity),
            (charity / after_tax) * Decimal::from(100)
        );
    }

    println!(
        "Minimum to bring all assets to target: {:}",
        decutil::format_dollars(&portfolio.minimum_addition_to_balance())
    );
    let contribution = get_contribution();

    // From those ideal allocations, identify the best way to invest a lump sum
    let balanced_portfolio = rebalance::optimally_allocate(portfolio, contribution);
    balanced_portfolio.describe_future_contributions();
}
