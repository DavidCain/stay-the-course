# About
This tool tells you the optimal way in which to rebalance your portfolio
if you are only buying or only selling securities.


## Process
### First step: acquire user preferences
Request desired asset allocation ratio from the user.
Supported categories:
    - US bonds
    - International bonds
    - US stocks
    - International Stocks
    - TIPS (inflation-protected securities)
    - Real Estate
    - US Treasury

### Second step: make recommendations
1. Parse a GNUCash file, identify current balance of all funds
  - Current balance is defined as last price * total shares
2. Categorize funds into categories
3. Determine current balances by category
4. Run the optimal lazy portfolio rebalancing algorithm
5. Output recommendations
6. (User executes trades)
