# Interface Map

## Entry and state model

```text
Main window
└── Top-left mode switch
    ├── LIVE (existing dashboard and live sidebar)
    └── BACKTEST (isolated replay dashboard)
        └── Bottom-left replay control block
            ├── Local symbol picker: EURUSD / NKE
            ├── UTC date-time input
            ├── Go
            ├── Cursor/status text
            ├── Backward jumps: -10m / -5m / -1m
            ├── Speed: 1x / 2x / 5x / 10x / 100x
            └── Forward jumps: +1m / +5m / +10m
```

## Target states

- LIVE selected.
- BACKTEST selected with default local symbol.
- NKE selected.
- EURUSD selected.
- Valid same-day UTC seek.
- Valid downloaded-date seek.
- Invalid date-time format.
- Valid format with unavailable date.
- First and last available day boundaries.
- End-of-day playback.
- Repeated speed/jump/mode actions.

## Excluded surfaces

- Existing live ticker table, exchange connectors, charts, menu bar, account panels, and trading widgets except for verifying that they are absent or inert in Backtest mode.
- Mobile and web responsive behavior; the product is a native desktop application.
- Real orders, real account changes, and external communication.
