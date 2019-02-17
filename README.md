_This tool is a work in progress, and a first project to learn Rust._

**Warning:** The author is neither a tax professional nor a retirement advisor.
Any statements contained within this document do not constitute legal, tax, or
investment advice.


# About
Long-term investors try to keep a mix of asset types in proportions that match
their risk preferences. Over time, prices fluctuate and the actual worth of
assets will inevitably change. This tool calculates the optimal way to invest a
fixed sum of money so that each asset type's value is as close as possible to
its desired ratio of the whole portfolio.

Broadly, this tool provides a means of ["staying the course"][stay_the_course]
through regular contributions into the right mutual funds.

## How it works
The tool accepts a few key inputs:

1. The path to a [GnuCash][gnucash] data file (in either XML or SQLite format)
2. The desired target allocation per asset type
3. The amount of money the investor intends to invest

With that information supplied, the tool will:

1. Use the contained price database to calculate the current worth of each investment fund
2. Classify each fund into an asset type
3. Sum up asset values by asset type, calculate the ratio of each asset class
   to the total portfolio value
4. Sequentially identify asset classes which have deviated most from their
   target, invest into those asset classes until they are as close to their
   target as the next furthest asset class (repeat until the desired
   contribution amount has been fully allocated to all funds)
5. Output the optimal contributions


# Background - target asset allocation
[Asset allocation][asset_allocation] is the process of reconciling one's risk
preferences and investment goals with a long-term strategy.

Stocks are generally understood to grant higher returns in the long run, at the
risk of greater volatility in the short run. Conversely, many consider bonds a
source of stable income, with lessened potential for long-term growth. Some
asset classes (such as [TIPS][TIPS]) may provide some insurance against changing
economic conditions, while other funds provide broad exposure to varied
economic sectors (adding diversity to a portfolio).

Any long-term investment strategy should be tailored to the risk preferences
of the investor. A young professional will likely desire a riskier portfolio
than somebody nearing retirement: The young professional is able to tolerate
short-term volatility in the hopes of greater returns. The retiree needs a
steady stream of income, and is willing to forego greater returns in exchange
for stability.

## Why use this strategy?
Rebalancing can be expensive. When selling taxable funds, an investor must
realize capital gains (or losses) in order to move money from one investment to
another. If shares have not been held for a sufficiently long period of time,
short-term capital gains may even be realized (at a potentially higher tax rate).

For residents of California, capital gains are taxed as normal income. If the
investor plans to retire in another state, they may desire to postpone the
realization of capital gains.

Selling shares adds some complexity to annual tax returns, since capital gains
must be reported to the IRS. A buy-and-hold investor may instead be able to go
years without selling any assets, avoiding the need to report realized gains.


## When rebalancing should be performed instead
Regular re-investment may be sufficient to keep [asset allocation][asset_allocation]
in line with targets. However, a number of scenarios might cause the ratio of
various asset classes to deviate too far from targets:

- Significant market changes (e.g. domestic or international expansion/contraction)
- Change in risk preferences (changing target weights for stocks, bonds, TIPS, etc.)
- Regular contributions being too small a fraction of the overall portfolio.

Only an individual investor can decide when their portfolio's composition has
deviated too far from its targets. At that time, it may become necessary to sell
overweighted assets and direct the proceeds to an underweighted asset class.

Taking dividends in cash (rather than automatically re-investing into the
originating fund) can help alleviate the need for rebalancing. The dividends
from overweighted funds may be transferred into underweighted funds.
Additionally, receiving dividends this way can help avoid a "wash sale" if the
investor plains to perform [tax loss harvesting][tax_loss_harvesting].


## Assumptions
- While this algorithm can handle any number of funds and corresponding asset classes,
  we do assume that the user employs a ["lazy portfolio"][lazy_portfolio]
  strategy (a portfolio in which desired allocations are spread across a small
  number of asset classes and underlying mutual funds). The GnuCash integration
  only considers assets whose underlying commodities are of type `FUND`.
- Current values are based on the last known price. The user must keep their
  price database current within GnuCash in order to get current estimates.

## Considerations this tool does not make
1. No differentiation is made between taxable and tax-advantaged accounts. It is up
   to the user to manage [efficient fund placement][tax_effecient_placement] and direct
   funds to the appropriate accounts.
2. No consideration is made of minimum investments: Investing in a new mutual fund often
   requires a minimum investment, but this tool does not account for that. The included
   example files show an investor building a diverse portfolio first before
   working towards bringing their various funds in line with target ratios.

## Major tasks outstanding
This project is very much a work in progress. Some key outstanding tasks:

- [ ] Automated build support
- [ ] Fix XML format parsing (only SQLite works at present)
- [ ] Optimize XML parsing (currently takes a couple seconds on a 20MB file)
- [ ] Command line interface
- [ ] Use a CSV (default included, but support user-provided) that maps ticker
      names to asset classes
- [ ] Warn when the last known price for a security is too old
- [ ] Return Result instead of just panicking on error conditions

## External resources
- [Optimal rebalancing][optimal_rebalancing]: The excellent web tool by Albert
  H. Mao. Provides a textual interface to lazily rebalance.
- [`rebalance-app`][rebalance-app] by Alberto Leal: another Rust implementation
  based off [Optimal rebalancing][optimal_rebalancing], but without GnuCash
  integration and relying on a different underlying type libraries. Probably better
  suited for people who don't make use of GnuCash. Also includes some features not
  present in this project!



[gnucash]: https://www.gnucash.org/
[optimal_rebalancing]: http://optimalrebalancing.tk
[rebalance-app]: https://github.com/dashed/rebalance-app

[TIPS]: https://en.wikipedia.org/wiki/United_States_Treasury_security#TIPS

[asset_allocation]: https://www.bogleheads.org/wiki/Asset_allocation
[stay_the_course]: https://www.bogleheads.org/blog/bogleheads-principles-stay-the-course/
[lazy_portfolio]: https://www.bogleheads.org/wiki/Lazy_portfolios#Three_fund_lazy_portfolios
[tax_loss_harvesting]: https://www.bogleheads.org/wiki/Tax_loss_harvesting
[tax_effecient_placement]: https://www.bogleheads.org/wiki/Tax-efficient_fund_placement
