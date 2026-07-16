# Focused Flow Test Matrix

| ID | Goal | Start state | Actions | Expected | Evidence |
| --- | --- | --- | --- | --- | --- |
| F-01 | Enter replay safely | LIVE | Click BACKTEST | Local-only replay dashboard; no live sidebar | Full screenshot |
| F-02 | Switch local symbol | BACKTEST | Select NKE, then EURUSD | DOM resets to selected local data and status identifies the cursor | Before/after screenshots |
| F-03 | Change speed | BACKTEST | Select 2x, 5x, 10x, 100x, 1x | Exactly one selected speed; cursor advances accordingly | Focused screenshots and observation |
| F-04 | Jump relatively | BACKTEST | Click each backward/forward jump | Cursor moves by requested amount and clamps safely at day boundaries | Focused screenshots |
| F-05 | Seek precisely | BACKTEST | Enter downloaded UTC date-time; click Go and press Return | Cursor moves to the requested session/time | Before/after screenshots |
| F-06 | Recover from invalid input | BACKTEST | Submit empty, malformed, and unavailable date | Clear corrective status; no crash or stale symbol | Error screenshots |
| F-07 | Survive impatient input | BACKTEST | Rapidly repeat speed, jump, Go, and mode controls | No crash, duplicate panes, stale selected state, or live/replay mix | Sequence evidence |
| F-08 | Return to live | BACKTEST | Click LIVE, then BACKTEST | Existing live dashboard returns; replay state remains isolated | Before/after screenshots |
| F-09 | Keyboard operation | BACKTEST | Tab through new controls; use Return where applicable | Predictable focus order and visible focus; seek can submit from keyboard | Focus sequence notes |

## Coverage rule

Only defects in the new controls or state transitions they directly trigger become findings. Pre-existing terminal defects are not triaged in this run.
