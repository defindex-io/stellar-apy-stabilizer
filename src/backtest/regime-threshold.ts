/**
 * Backtest REGIME_CHANGE_THRESHOLD for the fee-stabilizer's dual-signal gate.
 *
 * Usage: pnpm tsx src/backtest/regime-threshold.ts [csv-path]
 *
 * Input CSV columns required: ledger, timestamp, pps
 * (extra columns like apy_7d/apy_30d are ignored)
 *
 * What it measures
 * ----------------
 * At each historical point t with enough history, compute:
 *   longApy   = annualized return over the last LONG_DAYS  (e.g., 7d)
 *   shortApy  = annualized return over the last SHORT_DAYS (e.g., 1d)
 *   divergence = |shortApy / longApy − 1|
 *
 * The dual-signal gate would fire `act_on_short_regime` when divergence > τ.
 * To judge whether τ is well-tuned, look forward by LOOKAHEAD_DAYS and ask:
 * did longApy at t+lookahead actually move in the direction that the
 * short window was pointing at t? If yes → "correct" (real regime change).
 * If long reverted in the opposite direction → "reverted" (false alarm).
 *
 * The "baseline" column is the same accuracy computed over ALL points
 * (regardless of divergence). If a threshold's accuracy is barely above
 * baseline, the divergence signal isn't doing much filtering work.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';

const LONG_DAYS = 7;
const SHORT_DAYS = 1;
const LOOKAHEAD_DAYS = 3;
const THRESHOLDS = [0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.50];

// Skip points where |longApy| is below this. Tiny denominators amplify noise
// in the divergence ratio.
const MIN_LONG_APY_ABS = 0.005;
// Treat a future-vs-now long-APY delta below this as "no change" — neither a
// hit nor a miss, just regime stability.
const NO_CHANGE_EPSILON = 0.002;

const MS_PER_DAY = 86_400_000;
const GREGORIAN_YEAR_DAYS = 365.2425;

interface PpsRow {
  ledger: number;
  timestamp: Date;
  pps: number;
}

interface ComputedRow {
  ledger: number;
  timestamp: Date;
  pps: number;
  longApy: number;
  shortApy: number;
  divergence: number;
}

interface Scorecard {
  threshold: number;
  fires: number;
  correct: number;
  reverted: number;
  noChange: number;
  accuracy: number;
  baselineAccuracy: number;
}

function parseCsv(filePath: string): PpsRow[] {
  const text = fs.readFileSync(filePath, 'utf-8').trim();
  const lines = text.split('\n');
  const header = lines[0].split(',').map((h) => h.trim());
  const col = (name: string): number => {
    const i = header.indexOf(name);
    if (i === -1) throw new Error(`Missing column "${name}" in CSV header: ${header.join(', ')}`);
    return i;
  };
  const lIdx = col('ledger');
  const tIdx = col('timestamp');
  const pIdx = col('pps');

  const rows: PpsRow[] = [];
  for (let i = 1; i < lines.length; i++) {
    const cols = lines[i].split(',');
    if (cols.length <= Math.max(lIdx, tIdx, pIdx)) continue;
    const pps = parseFloat(cols[pIdx]);
    const ledger = parseInt(cols[lIdx], 10);
    if (!isFinite(pps) || !isFinite(ledger)) continue;
    rows.push({ ledger, timestamp: new Date(cols[tIdx]), pps });
  }
  rows.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());
  return rows;
}

function annualizedReturn(ppsStart: number, ppsEnd: number, days: number): number {
  if (ppsStart <= 0 || days <= 0) return 0;
  return Math.pow(ppsEnd / ppsStart, GREGORIAN_YEAR_DAYS / days) - 1;
}

/**
 * For each row t, find the latest earlier row whose timestamp is <=
 * (t − targetDays). Returns null when no such row exists (early in the series)
 * or when the actual window is < half the target (forces a real lookback).
 */
function findLookbackRow(
  rows: PpsRow[],
  index: number,
  cursor: number,
  targetDays: number,
): { row: PpsRow; cursor: number; actualDays: number } | null {
  const tMs = rows[index].timestamp.getTime();
  const targetMs = tMs - targetDays * MS_PER_DAY;
  let c = cursor;
  while (c + 1 < rows.length && rows[c + 1].timestamp.getTime() <= targetMs) c++;
  if (rows[c].timestamp.getTime() > targetMs) return null;
  const actualDays = (tMs - rows[c].timestamp.getTime()) / MS_PER_DAY;
  if (actualDays < targetDays * 0.5) return null;
  return { row: rows[c], cursor: c, actualDays };
}

function computeRows(rows: PpsRow[]): ComputedRow[] {
  const out: ComputedRow[] = [];
  let longCursor = 0;
  let shortCursor = 0;
  for (let i = 0; i < rows.length; i++) {
    const long = findLookbackRow(rows, i, longCursor, LONG_DAYS);
    const short = findLookbackRow(rows, i, shortCursor, SHORT_DAYS);
    if (!long || !short) continue;
    longCursor = long.cursor;
    shortCursor = short.cursor;

    const longApy = annualizedReturn(long.row.pps, rows[i].pps, long.actualDays);
    const shortApy = annualizedReturn(short.row.pps, rows[i].pps, short.actualDays);
    if (Math.abs(longApy) < MIN_LONG_APY_ABS) continue;

    const divergence = Math.abs(shortApy / longApy - 1);
    out.push({
      ledger: rows[i].ledger,
      timestamp: rows[i].timestamp,
      pps: rows[i].pps,
      longApy,
      shortApy,
      divergence,
    });
  }
  return out;
}

function scoreThreshold(computed: ComputedRow[], threshold: number): Scorecard {
  let fires = 0;
  let correct = 0;
  let reverted = 0;
  let noChange = 0;
  let baselineCorrect = 0;
  let baselineReverted = 0;
  let futureCursor = 0;

  for (let i = 0; i < computed.length; i++) {
    const row = computed[i];
    const futureMs = row.timestamp.getTime() + LOOKAHEAD_DAYS * MS_PER_DAY;
    while (
      futureCursor < computed.length &&
      computed[futureCursor].timestamp.getTime() < futureMs
    ) {
      futureCursor++;
    }
    if (futureCursor >= computed.length) break;
    const future = computed[futureCursor];

    const deltaActual = future.longApy - row.longApy;
    const deltaPredicted = row.shortApy - row.longApy;
    const isNoChange = Math.abs(deltaActual) < NO_CHANGE_EPSILON;
    const sameSign = Math.sign(deltaActual) === Math.sign(deltaPredicted);

    if (row.divergence > threshold) {
      fires++;
      if (isNoChange) noChange++;
      else if (sameSign) correct++;
      else reverted++;
    }

    if (!isNoChange) {
      if (sameSign) baselineCorrect++;
      else baselineReverted++;
    }
  }

  const decided = correct + reverted;
  const baselineDecided = baselineCorrect + baselineReverted;
  return {
    threshold,
    fires,
    correct,
    reverted,
    noChange,
    accuracy: decided > 0 ? correct / decided : 0,
    baselineAccuracy: baselineDecided > 0 ? baselineCorrect / baselineDecided : 0,
  };
}

function formatPercent(x: number): string {
  return (x * 100).toFixed(1).padStart(5) + '%';
}

function printDistribution(computed: ComputedRow[]): void {
  const sorted = [...computed].map((r) => r.divergence).sort((a, b) => a - b);
  const at = (q: number): number => sorted[Math.floor(sorted.length * q)];
  console.log('Divergence distribution across all eligible points:');
  console.log(`  p50: ${at(0.5).toFixed(3)}  p75: ${at(0.75).toFixed(3)}  p90: ${at(0.9).toFixed(3)}  p95: ${at(0.95).toFixed(3)}  p99: ${at(0.99).toFixed(3)}`);
  console.log('');
}

function main(): void {
  const argPath = process.argv[2] ?? './meru-pps-historical.csv';
  const csvPath = path.resolve(argPath);
  console.log(`Loading ${csvPath}`);

  const rows = parseCsv(csvPath);
  if (rows.length === 0) {
    console.error('No rows parsed.');
    process.exit(1);
  }
  console.log(
    `Parsed ${rows.length} rows  ·  ${rows[0].timestamp.toISOString().slice(0, 10)} → ${rows[rows.length - 1].timestamp.toISOString().slice(0, 10)}`,
  );

  const computed = computeRows(rows);
  console.log(`Eligible points (both windows valid, |long| ≥ ${MIN_LONG_APY_ABS}): ${computed.length}\n`);
  if (computed.length === 0) {
    console.error('No points met the window requirements. Need ≥ LONG_DAYS of history.');
    process.exit(1);
  }

  printDistribution(computed);

  console.log(
    `Lookahead: ${LOOKAHEAD_DAYS}d  ·  no-change ε: ${NO_CHANGE_EPSILON}  ·  long=${LONG_DAYS}d short=${SHORT_DAYS}d\n`,
  );
  console.log(
    'threshold  fires   correct  reverted  no-change   accuracy   baseline   lift',
  );
  console.log(
    '─────────  ─────   ───────  ────────  ─────────   ────────   ────────   ────',
  );
  for (const t of THRESHOLDS) {
    const s = scoreThreshold(computed, t);
    const lift = s.accuracy - s.baselineAccuracy;
    console.log(
      `  ${t.toFixed(2)}     ${String(s.fires).padStart(5)}    ${String(s.correct).padStart(5)}     ${String(s.reverted).padStart(5)}     ${String(s.noChange).padStart(5)}     ${formatPercent(s.accuracy)}    ${formatPercent(s.baselineAccuracy)}    ${(lift * 100).toFixed(1).padStart(4)}pp`,
    );
  }
}

main();
