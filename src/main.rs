extern crate rust_decimal;

use self::rust_decimal::Decimal;
use std::io;

mod assets;
mod gnucash;
mod rebalance;

use gnucash::Book;

fn get_contribution() -> Decimal {
    let mut contribution = String::new();

    println!("How much to contribute or withdraw?");
    io::stdin()
        .read_line(&mut contribution)
        .expect("Failed to read line");

    contribution.trim().parse().expect("Please type a number!")
}

fn main() {
    println!("Parsing Gnucash datafile...");
    let book = Book::from_sqlite_file("example.sqlite3");
    //let book = Book::from_xml_file("example.gnucash");

    // Identify our ideal allocations (percentages by asset class, summing to 100%)
    let ideal_allocations = rebalance::ideal_allocations();

    let portfolio = book.portfolio_status(ideal_allocations);

    println!(
        "\nCurrent portfolio totals ${:.0}",
        &portfolio.current_value()
    );
    let contribution = get_contribution();

    // From those ideal allocations, identify the best way to invest a lump sum
    let balanced_portfolio = rebalance::optimally_allocate(portfolio, contribution);
    balanced_portfolio.describe_future_contributions();
}
