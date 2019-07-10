extern crate rusqlite;
extern crate rust_decimal;

use self::rusqlite::{Connection, NO_PARAMS};
use self::rust_decimal::Decimal;

pub struct Stats {
    conn: Connection,
}

impl Stats {
    /// Open a connection to a SQLite accounting file, provide statistics!
    pub fn new(filename: &str) -> Stats {
        let conn = Connection::open(filename).expect("Could not open file");
        Stats { conn }
    }

    /// Retrieve the guid of an account under Root -> Expenses
    fn top_level_expense_account(&self, name: &str) -> rusqlite::Result<String> {
        let sql = format!(
            "WITH root_account AS (
               SELECT guid
                 FROM accounts
                WHERE name = 'Root Account'
                  AND account_type = 'ROOT'
             ), root_expenses AS (
               SELECT guid
                 FROM accounts
                WHERE name = 'Expenses'
                  AND account_type = 'EXPENSE'
                  AND parent_guid = (SELECT guid from root_account)
             )
             SELECT guid
               FROM accounts
              WHERE name = '{name}'
                AND account_type = 'EXPENSE'
                AND parent_guid = (SELECT guid from root_expenses);
            ",
            name = name
        );
        let mut stmt = (&self.conn).prepare(&sql)?;
        let mut guids = stmt.query_map(NO_PARAMS, |row| {
            let taxes_guid: String = row.get(0);
            taxes_guid
        })?;
        guids.next().expect("Can't find Expenses account!")
    }

    /// Add up the values for all transactions in the given accounts
    ///
    /// # Arguments
    ///  - `ctes` - Common table expressions to be placed before the main `SELECT`
    ///  - `where_clause` - a clause for filtering on the `accounts` table
    fn sum_splits(&self, ctes: &str, where_clause: &str) -> rusqlite::Result<Decimal> {
        let sql = format!(
            "{ctes}
             SELECT value_num, value_denom
               FROM splits
              WHERE account_guid IN
                    (SELECT guid FROM accounts WHERE {where_clause})",
            ctes = ctes,
            where_clause = where_clause
        );

        let mut stmt = (&self.conn).prepare(&sql)?;
        let rows = stmt.query_map(NO_PARAMS, |row| {
            let value_num: i64 = row.get(0);
            let value_denom: i64 = row.get(1);
            Decimal::from(value_num) / Decimal::from(value_denom)
        })?;

        rows.sum()
    }

    /// Sum all transactions under the account and any account's children
    fn sum_all_transactions_in(&self, root_guid: &str) -> rusqlite::Result<Decimal> {
        let ctes = format!(
            "WITH RECURSIVE
               child_accounts(last_parent) AS (
                 -- (Not concerned about SQL injection here, as guids are just hex chars)
                 VALUES('{root_guid}')
                  UNION
                 SELECT guid
                   FROM accounts, child_accounts
                  WHERE accounts.parent_guid = child_accounts.last_parent
             )",
            root_guid = root_guid
        );
        self.sum_splits(&ctes, "guid IN child_accounts")
    }

    /// Sum all income (before any taxes are applied)
    ///
    /// Note that income will be _positive_, despite the fact that dual-entry
    /// accounting typically regards income as negatively signed.
    fn income_before_taxes(&self) -> rusqlite::Result<Decimal> {
        match self.sum_splits("", "account_type='INCOME'") {
            // Income is recorded as negative, but we want to consider it positive!
            Ok(total) => Ok(-total),
            x => x,
        }
    }

    /// Sum all taxes paid out of income
    ///
    /// "Taxes" are any transactions found in Root -> Expenses -> Taxes
    /// In my accounting system, that includes:
    /// - Federal & state income tax
    /// - Social Security
    /// - Medicare
    fn taxes_paid(&self) -> rusqlite::Result<Decimal> {
        let taxes_guid = self.top_level_expense_account("Taxes")?;
        self.sum_all_transactions_in(&taxes_guid)
    }

    /// Calculate the total income, less any taxes paid
    ///
    /// Note that the return value is expected to be _positive_ (unless the amount
    /// paid in taxes somehow exceeds total income).
    pub fn after_tax_income(&self) -> rusqlite::Result<Decimal> {
        Ok(self.income_before_taxes()? - self.taxes_paid()?)
    }

    /// Sum value of all contributions to charity
    pub fn charitable_giving(&self) -> rusqlite::Result<Decimal> {
        let charity_guid = self.top_level_expense_account("Charity")?;
        self.sum_all_transactions_in(&charity_guid)
    }
}
