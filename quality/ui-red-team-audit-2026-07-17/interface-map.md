# Interface Map

```text
BACKTEST dashboard
├── CScalp DOM
│   ├── top-left Status timeline
│   │   ├── previous (inactive)
│   │   ├── current (active)
│   │   └── next position (inactive, future value hidden)
│   └── price ladder
│       ├── auto-follow when idle
│       ├── pinned absolute rows while hovered
│       └── pinned absolute rows after manual scroll until reset
└── bottom-left Replay controls
    ├── local symbol
    ├── Jump to: Day / Month / Year / HH:MM:SS / venue timezone / Go
    ├── relative jumps
    └── Speed: pause / 1x / 2x / 5x / 10x / 100x
```

## Target states

- NKE with New York daylight time and exchange phase available.
- EURUSD with Chicago daylight time and exchange phase available.
- Paused at a fixed cursor.
- Resumed at each speed.
- Hovered while the best bid changes.
- Manually scrolled, then reset to auto-follow.
- Valid available date and invalid selector/time combinations.

## Exclusions

Existing live trading, menus, charts, connectivity, order entry, and other panels are not findings unless a new replay control directly corrupts them.
