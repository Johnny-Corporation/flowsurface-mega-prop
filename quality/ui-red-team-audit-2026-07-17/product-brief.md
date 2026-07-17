# Focused Product Brief: Full-Book Replay Additions

## Product and user

Flowsurface is a native macOS trading terminal. This audit covers only the replay additions made on 17 July 2026 for an order-flow trader reviewing locally downloaded NKE and Euro FX data.

## First success moment

In BACKTEST mode, the trader can pause, choose day/month/year and venue-local time, see the current exchange phase, and inspect NKE without the price row under the pointer moving.

## Success criteria

- Pause is immediately left of `1x`, visibly selected, and stops the cursor.
- Day, month, and year are separate, readable, and only allow locally available dates.
- Cursor and seek input use New York time for NKE and Chicago time for Euro FX.
- The status timeline shows previous/current/next positions without exposing a future unscheduled state.
- Hover and manual scroll pin absolute DOM price rows while quantities continue updating.
- Full replay continues after pause, seek, speed changes, and symbol switches without stale state.

## Consequence of mistakes

Mistakes can create false backtest conclusions through future leakage, incorrect time conversion, skipped order events, or a moving price axis that looks like liquidity is following price.

## Audit boundary

Included: pause/speed row, separate date selectors, local-time labels, three-node status timeline, full-book display behavior, and hover/manual price-axis anchoring. Existing menus, charts, account panels, live connectors, and unrelated DOM defects are excluded.
