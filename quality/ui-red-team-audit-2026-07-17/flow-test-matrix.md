# Focused Flow Test Matrix

| ID | Goal | Actions | Expected evidence |
| --- | --- | --- | --- |
| N-01 | Pause and resume | Pause, wait, select 1x and 100x | Cursor fixed while paused; resumes once; selected state is clear |
| N-02 | Select a date | Choose Day, Month, Year separately and submit time | Human-readable controls; correct local cursor |
| N-03 | Reject bad time | Empty, partial, invalid, and boundary times | Corrective copy; no crash or stale seek |
| N-04 | Preserve time meaning | Switch NKE/EURUSD and seek same displayed time | NKE shows EDT/EST; EURUSD shows CDT/CST |
| N-05 | Read status | Seek across a known phase change | Previous/current update; next state remains hidden |
| N-06 | Pin hover row | Hold pointer over a price while replay advances | Absolute price under pointer does not move; quantities can change |
| N-07 | Pin manual axis | Scroll, replay, then reset | Rows remain pinned until reset; auto-follow resumes afterward |
| N-08 | Stress controls | Rapid pause/speed/jump/date/symbol interactions | No crash, duplicate state, skipped control selection, or live leakage |
| N-09 | Inspect full book | Scroll well beyond ten levels on NKE and EURUSD | More than ten live levels are available without a configured depth cap |

Only defects introduced by these additions are triaged.
