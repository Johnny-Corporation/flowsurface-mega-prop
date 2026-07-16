# Focused Product Brief: Databento Replay Controls

## Product and users

Flowsurface is a native macOS trading terminal. This focused audit covers the new local Databento replay workflow for a trader or strategy developer who needs to inspect historical order-book behavior without receiving live-market noise or sending live orders.

## Primary goal

Enter Backtest mode, choose a locally downloaded symbol, navigate to a precise UTC time, and replay the local order book and trades at a useful speed.

## First success moment

The user switches from LIVE to BACKTEST, sees only `NKE` and `EURUSD`, and can tell that historical data is advancing.

## Success criteria

- The mode and selected state are obvious within five seconds.
- Backtest mode exposes only locally downloaded symbols.
- Switching modes cannot leak live exchange events into replay or submit live orders.
- Speed, relative jumps, and absolute UTC seek are discoverable and predictable.
- Invalid or unavailable dates explain how to recover.
- Repeated clicks and mode changes do not crash, duplicate panes, or leave stale state.

## Consequence of mistakes

The main risks are false research conclusions from mixed live/replay state, accidental live trading, selecting the wrong time, and losing orientation during fast playback. No real order submission is authorized during this audit.

## Audit boundary

Included: the LIVE/BACKTEST switch, local symbol picker, UTC field, Go action, status copy, jump buttons, speed buttons, and their immediately adjacent replay states. Existing chart, sidebar, menu, panel, and live-trading defects are excluded unless a new control directly causes them.
