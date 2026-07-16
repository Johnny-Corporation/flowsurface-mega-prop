# Permissions and Safety Boundary

## Authorized

- Operate the local Flowsurface build through the macOS UI.
- Use the locally downloaded NKE and Euro FX replay fixtures.
- Switch between LIVE and BACKTEST without placing orders.
- Enter synthetic UTC values into the replay field.
- Capture and annotate screenshots of the new controls.
- Resize the application window and perform repeated, non-destructive control actions.

## Prohibited

- Submit, cancel, or modify a real order.
- Modify exchange credentials, billing, account settings, or permissions.
- Expand the audit into unrelated existing UI defects.
- Include tokens, account identity, billing details, or other confidential data in evidence.

## Stop conditions

- Any path that appears capable of sending a live order.
- Any prompt that requests credentials or changes external state.
- Any test that would destroy or overwrite the downloaded raw dataset.

## Environment

- Native macOS desktop build from `johnny/feat/databento-l2-replay`.
- Dark theme, normal system text scale, local data, ordinary network state.
- Exact window dimensions are recorded with captured evidence.
