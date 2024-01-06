[![Build Status](https://github.com/DavidCain/stay-the-course/actions/workflows/ci.yml/badge.svg)](https://github.com/DavidCain/stay-the-course/actions)

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

## Demo
Just clone and `cargo run` to see a demonstration with a real GnuCash database:

```
$ git clone git@github.com:DavidCain/stay-the-course.git
$ cd stay-the-course
$ cargo run
US stocks: $10032 (🎯 42.68%)
  - FZROX: $5120.56 (485.3619 x $10.55)
  - VTSAX: $4911.94 (66.5305 x $73.83)
International stocks: $7749 (🎯 34.14%)
  - VTIAX: $7749.70 (273.7445 x $28.31)
US bonds: $3393 (🎯 14.63%)
  - VBTLX: $3393.63 (311.0576 x $10.91)
REIT: $3330 (🎯 8.53%)
  - VGSLX: $3330.72 (26.1438 x $127.40)
Portfolio total: $24506

Worth at retirement (Assuming 7% growth):
 - 34: $24506  SWR: $980
 - 50: $69858  SWR: $2794
 - 55: $97975  SWR: $3919
 - 60: $137434  SWR: $5497
 - 65: $192750  SWR: $7710

After-tax income: $49700
Charitable giving: $5000 (10% of after-tax income)
How much to contribute or withdraw?
2000
Contribute the following amounts:
 - International stocks: $903.07
   31.62% -> 32.64% (🎯 34.14%) Δ [7.3% -> 4.4%]
 - US bonds: $313.46
   13.84% -> 13.98% (🎯 14.63%) Δ [5.3% -> 4.4%]
 - US stocks: $783.46
   40.93% -> 40.80% (🎯 42.68%) Δ [4.0% -> 4.4%]
 - REIT: $0.00
   13.59% -> 12.56% (🎯 8.53%) Δ [-59.2% -> -47.1%]
```

### Sample GnuCash accounting records

In `example/` are two (identical) sample files in XML and sqlite3 format. Each
may be opened with [GnuCash 3][gnucash]. The transactions, security prices, and
account balances contained within are the basis of rebalancing logic:

![GnuCash user interface for included sample files][img-gnucash-interface]

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

## Fetching quotes from 3rd party APIs
I'm using the AlphaVantage free API. To use it, make sure that:

1. `ALPHAVANTAGE_API_KEY` is an available env var. ([Get an API key][av-api-key] first)
2. `update_prices = true` is set in `[gnucash]` within `config.toml`

When configured, this ensures that the latest stock prices per fund
are incorporated into the allocation recommendations.


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
[Rebalancing][rebalancing] can be expensive. When selling taxable funds, an
investor must realize capital gains (or losses) in order to move money from one
investment to another. If shares have not been held for a sufficiently long
period of time, short-term capital gains may even be realized (at a potentially
higher tax rate).

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
   to the user to manage [efficient fund placement][tax_efficient_placement] and direct
   funds to the appropriate accounts.
2. No consideration is made of minimum investments: Investing in a new mutual fund often
   requires a minimum investment, but this tool does not account for that. The included
   example files show an investor building a diverse portfolio first before
   working towards bringing their various funds in line with target ratios.

## Major tasks outstanding
This project is very much a work in progress. Some key outstanding tasks:

- [ ] Optimize XML parsing (currently takes a couple seconds on a 20MB file)
- [ ] Command line interface
- [ ] Return Result instead of just panicking on error conditions

## External resources
- [Optimal rebalancing (archive)][optimal_rebalancing]: Though it has since
  been taken down (it lived at `http://optimalrebalancing.tk`, now spam), this
  was an excellent web tool by Albert H. Mao. It provided a textual interface
  to lazily rebalance.
- [`rebalance-app`][rebalance-app] by Alberto Leal: another Rust implementation
  based off [Optimal rebalancing][optimal_rebalancing], but without GnuCash
  integration and relying on a different underlying type libraries. Probably better
  suited for people who don't make use of GnuCash. Also includes some features not
  present in this project!



[gnucash]: https://www.gnucash.org/
[optimal_rebalancing]: https://archive.ph/IUimB
[rebalance-app]: https://github.com/dashed/rebalance-app

[TIPS]: https://en.wikipedia.org/wiki/United_States_Treasury_security#TIPS

[asset_allocation]: https://www.bogleheads.org/wiki/Asset_allocation
[rebalancing]: https://www.bogleheads.org/wiki/Rebalancing
[stay_the_course]: https://www.bogleheads.org/blog/bogleheads-principles-stay-the-course/
[lazy_portfolio]: https://www.bogleheads.org/wiki/Lazy_portfolios#Three_fund_lazy_portfolios
[tax_loss_harvesting]: https://www.bogleheads.org/wiki/Tax_loss_harvesting
[tax_efficient_placement]: https://www.bogleheads.org/wiki/Tax-efficient_fund_placement
[av-api-key]: https://www.alphavantage.co/support/#api-key


[img-gnucash-interface]: https://github.com/DavidCain/stay-the-course/blob/master/images/gnucash_interface.png
