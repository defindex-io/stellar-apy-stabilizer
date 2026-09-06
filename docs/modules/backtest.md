# Backtest Module

> **Living document.** Read this before modifying the module. Update it in the same change whenever the module's behavior, endpoints, files, or dependencies change.

**Source:** `src/backtest/` · **Last verified:** 2026-09-04

## Purpose

An offline analysis script that scores candidate values for a divergence threshold used by a proposed dual-signal fee gate. It reads a price-per-share CSV, compares a short APY window against a long one at every point, and measures how often a large divergence actually predicted the long window's later move. Pure analysis: no network, no database, no on-chain writes.

## Structure

| File | Purpose |
|---|---|
| `regime-threshold.ts` | The whole script: CSV parse, window math, threshold scoring, table output. |

Default input is `pps-historical.csv` at the repo root.

## Public surface

A CLI script. No exports. `pnpm backtest:regime` (`package.json:12`), or `tsx src/backtest/regime-threshold.ts [csv-path]` per the usage comment at `regime-threshold.ts:4`.

Required CSV columns: `ledger`, `timestamp`, `pps` (`regime-threshold.ts:79`). Extra columns are ignored.

## Key methods

- **`parseCsv(filePath)`** (`regime-threshold.ts:70`) — resolves column indices by header name and throws a named error on a missing column. Skips rows with non-finite `pps` or `ledger`, then sorts by timestamp.
- **`findLookbackRow(rows, index, cursor, targetDays)`** (`regime-threshold.ts:106`) — advances a monotonic cursor to the latest row at or before `t - targetDays`. Returns `null` when the available window is under half the target, which forces a genuine lookback instead of a degenerate one-sample window.
- **`computeRows(rows)`** (`regime-threshold.ts:122`) — produces `longApy`, `shortApy` and `divergence = |short/long - 1|` per eligible point. Drops points where `|longApy| < MIN_LONG_APY_ABS` because a tiny denominator makes the ratio meaningless.
- **`scoreThreshold(computed, threshold)`** (`regime-threshold.ts:150`) — looks forward `LOOKAHEAD_DAYS` and classifies each firing as correct, reverted, or no-change. Also computes a baseline accuracy over *all* points so you can see whether the divergence filter earns its keep. The `lift` column in the output is `accuracy - baselineAccuracy`.

## Dependencies

- Node built-ins only: `node:fs`, `node:path` (`regime-threshold.ts:27`). No npm runtime dependency, no env vars.
- Input data: `pps-historical.csv` at the repo root, or a path passed as `argv[2]`.

## Gotchas and invariants

- **The gate this script tunes is not implemented.** Nothing in `src/stabilizer/` references a divergence or regime threshold. The live controller uses only the single-window `CONTROLLER_APY_WINDOW_DAYS = 1` path (`src/stabilizer/constants.ts:32`). Treat the output as exploratory input to a design decision, not as documentation of shipped behavior.
- **All tuning is compile-time.** `LONG_DAYS`, `SHORT_DAYS`, `LOOKAHEAD_DAYS`, `THRESHOLDS`, `MIN_LONG_APY_ABS`, `NO_CHANGE_EPSILON` are module constants (`regime-threshold.ts:30`). Changing a scenario means editing the file.
- **The default CSV path is relative to the current working directory**, not to the script (`regime-threshold.ts:217` uses `"./pps-historical.csv"` before `path.resolve`). Run it from the repo root or pass an absolute path.
- **`scoreThreshold` breaks out of the loop once the lookahead runs off the end of the series** (`regime-threshold.ts:168`), so the last `LOOKAHEAD_DAYS` of data contribute nothing to any score.
- **Annualization matches the production math**: `365.2425` days per year in both `annualizedReturn` (`regime-threshold.ts:98`) and `calculateLiveGrossApy` (`src/stabilizer/apy-calculation.ts:129`). Keep them in sync if either changes.
- **`pps-historical.csv` is committed and large** (527,859 bytes). It is data, not code; do not regenerate it casually, and do not assume it corresponds to any particular vault. Which vault it was exported from, and by what: _TBD, unverified_ — nothing in the repo records it.

## Testing

No automated tests. The script is its own harness: run it and read the table. A regression check is to confirm the baseline accuracy column stays stable across changes to the window math.
