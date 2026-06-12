# V6 - Hybrid Adaptive TV+PSY+ATR

## Overview
V6 is a new strategy that combines the strongest parts of the current adaptive
family without changing V3, V3.1, or V5.

- V3.1 trade-value signal base: `close * volume` in KRW
- V5 dual PSY confirmation: `psy_hour` and `psy_day` must both confirm
- ATR risk exits: fixed ATR stop and ATR trailing stop
- Optional ADX filter: `v6_min_adx = 0` disables it

## Signal Logic
V6 uses V3.1's state machine and `v31_*` trade-value parameters. The single
`v31_buy_psy_*` filter is not used. Instead, V6 uses V5's dual PSY parameters:

- Buy confirmation: `tv < set_trade_value * buy_decay`
  and `psy_hour < buy_psy_hour`
  and `psy_day < buy_psy_day`
- Sell confirmation: `tv < set_trade_value * sell_decay`
  and `psy_hour > sell_psy_hour`
  and `psy_day > sell_psy_day`

## Risk Exits
After `v31_min_hold_bars`, V6 may sell through existing V3.1 exits or the new
ATR exits:

- `v6_atr_stop_mult`: sell if price falls below `buy_price - ATR * multiplier`
- `v6_atr_trail_mult`: sell if price falls below `highest_since_buy - ATR * multiplier`
- `v6_min_adx`: require ADX at or above this value for buys; `0` disables it

## Parameters
V6 exposes:

- Most `v31_*` parameters from V3.1
- `v5_*` dual PSY parameters from V5
- `v6_atr_stop_mult`
- `v6_atr_trail_mult`
- `v6_min_adx`

`v31_buy_psy_lo`, `v31_buy_psy_hi`, and `v31_buy_psy_pow` are intentionally not
exposed for V6 because dual PSY replaces the single day-PSY gate.

## Suggested Use
Use V6 as the main experimental successor to V3.1, not as a drop-in winner.
Run NSGA-II optimization and compare it against V3.1 and V5 in paper sessions
using return, max drawdown, trade count, win rate, and average holding time.
