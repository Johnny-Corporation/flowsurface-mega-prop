# Permissions and Safety Boundary

## Authorized

- Operate the local Flowsurface build in BACKTEST mode.
- Use local NKE and Euro FX replay data.
- Seek, pause, change speed, scroll, hover, reset, resize, and capture screenshots.
- Fuzz only the new date/time controls with non-destructive values.

## Prohibited

- Place, cancel, or modify a real order.
- Change credentials, connections, billing, account settings, or downloaded raw data.
- Turn unrelated existing interface defects into findings.
- Capture tokens, account identity, or confidential billing details.

## Stop conditions

- A test appears capable of submitting a live order.
- The app is not clearly in BACKTEST mode.
- A test would overwrite or delete raw Databento files.

## Environment

- Native macOS build from `johnny/feat/databento-l2-replay`.
- Dark theme, local replay data, ordinary desktop window.
- Status evidence begins only after purchased Status files are imported.
