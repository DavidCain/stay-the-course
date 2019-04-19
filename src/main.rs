#[macro_use]
extern crate serde_derive;
extern crate chrono;
extern crate num;
extern crate rust_decimal;

use self::chrono::{Datelike, Local, NaiveDate};
use self::rust_decimal::Decimal;
use std::cmp;
use std::io;

mod allocation;
mod assets;
mod compounding;
mod gnucash;
mod rebalance;
mod stats;

use gnucash::Book;

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
            " - {}: ${:.0}  SWR: ${:.0}",
            retirement_age,
            future_total,
            compounding::safe_withdrawal_income(future_total)
        );
    }

    let today = Local::now().date().naive_local();
    summarize(today, birthday, portfolio_total);

    let approx_age = today.year() - birthday.year(); // Could be this age, or one year younger
    let start_age = cmp::max(50, approx_age + 5);

    let retirement_ages = (start_age)..=(start_age + 15);
    for age in retirement_ages.step_by(5) {
        let year = birthday.year() + age;
        let day_of_retirement = NaiveDate::from_ymd(year, birthday.month(), birthday.day());
        let future_total = compounding::compound(portfolio_total, real_apy, day_of_retirement);
        summarize(day_of_retirement, birthday, future_total);
    }
    println!("");
}

fn main() {
    let sqlite_file = "example.sqlite3";
    let book = Book::from_sqlite_file(sqlite_file);
    //let book = Book::from_xml_file("example.gnucash");

    // Identify our ideal allocations (percentages by asset class, summing to 100%)
    let birthday = NaiveDate::from_ymd(1960, 1, 1);
    let bond_allocation = allocation::bond_allocation(birthday, 120);
    let ideal_allocations = allocation::core_four(bond_allocation);

    let asset_classifications =
        assets::AssetClassifications::from_csv("data/classified.csv").unwrap();
    let portfolio = book.portfolio_status(asset_classifications, ideal_allocations);

    println!("{:}\n", portfolio);

    summarize_retirement_prospects(birthday, portfolio.current_value(), 0.07);

    let sql_stats = stats::Stats::new(sqlite_file);
    println!(
        "After-tax income: ${:.0}",
        sql_stats.after_tax_income().unwrap()
    );

    let contribution = get_contribution();

    // From those ideal allocations, identify the best way to invest a lump sum
    let balanced_portfolio = rebalance::optimally_allocate(portfolio, contribution);
    balanced_portfolio.describe_future_contributions();
}
