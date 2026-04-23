use crate::models::market::MarketData;
use crate::models::trading::SignalEvent;

/// Map (position, buy_sign, sell_sign, prev_position) to the human-readable
/// signal name used by the legacy timeline UI (`ready / buy ready / buy / hold
/// / sell ready / sell`). Ported from NetTradingEngine.DetermineSignalType.
pub fn determine_signal_type(position: i32, buy_sign: i32, sell_sign: i32, prev_position: i32) -> &'static str {
    if position == 1 && prev_position == 0 {
        return "buy";
    }
    if position == 0 && prev_position == 1 {
        return "sell";
    }
    if position == 0 {
        if buy_sign == 1 {
            return "buy ready";
        }
        return "ready";
    }
    if sell_sign == 1 {
        return "sell ready";
    }
    "hold"
}

/// Append a `SignalEvent` when the transition type differs from the previous one.
pub fn push_signal(
    log: &mut Vec<SignalEvent>,
    previous_type: &mut String,
    index: usize,
    data: &[MarketData],
    position: i32,
    buy_sign: i32,
    sell_sign: i32,
    prev_position: i32,
) {
    let current = determine_signal_type(position, buy_sign, sell_sign, prev_position);
    if current != previous_type.as_str() {
        let candle = &data[index].candle;
        log.push(SignalEvent {
            index,
            timestamp: candle.timestamp.to_rfc3339(),
            signal_type: current.to_string(),
            price: candle.close,
            position,
        });
        *previous_type = current.to_string();
    }
}
