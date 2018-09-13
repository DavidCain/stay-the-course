extern crate decimal;

use self::decimal::d128;

mod assets;
mod gnucash;
mod rebalance;

use gnucash::Book;

fn main() {
    let book = Book::from_file("example.gnucash");

    // Identify our ideal allocations (percentages by asset class, summing to 100%)
    let ideal_allocations = rebalance::ideal_allocations();

    // From those ideal allocations, identify the best way to invest a lump sum
    let portfolio = book.portfolio_status(ideal_allocations);

    let balanced_portfolio = rebalance::optimally_allocate(portfolio, d128!(3000));
    balanced_portfolio.describe_future_contributions();
}
