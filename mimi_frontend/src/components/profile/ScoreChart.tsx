import { useState } from 'react';
import type { Language, Point } from '../../data/profile';
import { DAY_MS, formatDate, monthLabel, valueAt } from '../../data/profile';

/* Every language on one set of axes, the whole reason for scoring them the
   same way, since two years of Spanish and six weeks of French are otherwise
   not comparable at all. The y-axis is the score itself, not a letter band:
   the score is the thing that moves day to day, and watching it move is the
   point of the graph.

   A plain SVG on a fixed viewBox, scaled by CSS. Nothing here animates. */

const RANGES = ['1M', '3M', '6M', 'YTD', '1Y', 'ALL'] as const;
type Range = (typeof RANGES)[number];

/* a wide, flat frame: a score history is a long time series, so a landscape
   plot reads it better and keeps the chart from towering over the page */
const W = 960;
const H = 230;
/* room on the right for the score labels, which sit outside the plot; almost
   none on the left, since nothing is written there */
const PAD = { top: 14, right: 46, bottom: 26, left: 8 };
const PW = W - PAD.left - PAD.right;
const PH = H - PAD.top - PAD.bottom;

function rangeStart(range: Range, earliest: number, today: number): number {
  switch (range) {
    case '1M':
      return today - 30 * DAY_MS;
    case '3M':
      return today - 91 * DAY_MS;
    case '6M':
      return today - 182 * DAY_MS;
    case 'YTD':
      return Date.UTC(new Date(today).getUTCFullYear(), 0, 1);
    case '1Y':
      return today - 365 * DAY_MS;
    default:
      return earliest;
  }
}

/** round score gridlines, spaced so there are never more than about six */
function scoreTicks(min: number, max: number): number[] {
  const span = max - min;
  const step = span > 1600 ? 500 : span > 800 ? 250 : span > 320 ? 100 : span > 120 ? 50 : 25;
  const out: number[] = [];
  for (let v = Math.ceil(min / step) * step; v <= max; v += step) out.push(v);
  return out;
}

/** date gridlines: days for a short window, months for a season, years for a life */
function dateTicks(t0: number, t1: number): { t: number; label: string }[] {
  const days = (t1 - t0) / DAY_MS;

  if (days <= 80) {
    const out: { t: number; label: string }[] = [];
    for (let t = t1; t > t0; t -= 14 * DAY_MS) {
      out.unshift({ t, label: `${new Date(t).getUTCDate()} ${monthLabel(t)}` });
    }
    return out;
  }

  const months: number[] = [];
  const d = new Date(t0);
  let y = d.getUTCFullYear();
  let m = d.getUTCMonth();
  if (Date.UTC(y, m, 1) < t0) m += 1;
  while (Date.UTC(y, m, 1) <= t1) {
    months.push(Date.UTC(y, m, 1));
    if (++m > 11) {
      m = 0;
      y += 1;
    }
  }

  const yearly = months.length > 20;
  const kept = yearly ? months.filter((t) => new Date(t).getUTCMonth() === 0) : months;
  const stride = Math.max(1, Math.ceil(kept.length / 7));
  return kept
    .filter((_, i) => i % stride === 0)
    .map((t) => ({ t, label: yearly ? String(new Date(t).getUTCFullYear()) : monthLabel(t) }));
}

/** the samples inside the window, with the score on the left edge carried in
    so a line that predates the window still starts at the axis */
function clip(points: Point[], t0: number): Point[] {
  const inside = points.filter((p) => p.t >= t0);
  const carried = valueAt(points, t0);
  return carried !== null && (inside.length === 0 || inside[0].t > t0)
    ? [{ t: t0, v: carried }, ...inside]
    : inside;
}

interface Props {
  languages: Language[];
  hidden: string[];
  /** the right-hand edge, as the server reckons today (see Profile.today) */
  today: number;
}

export default function ScoreChart({ languages, hidden, today }: Props) {
  const [range, setRange] = useState<Range>('ALL');
  const [hover, setHover] = useState<number | null>(null);

  const earliest = Math.min(...languages.map((l) => l.since));
  const t0 = Math.max(rangeStart(range, earliest, today), earliest);
  const t1 = today;

  const series = languages
    .filter((l) => !hidden.includes(l.id))
    .map((lang) => ({ lang, points: clip(lang.points, t0) }));

  const values = series.flatMap((s) => s.points.map((p) => p.v));
  const min = values.length ? Math.min(...values) : 400;
  const max = values.length ? Math.max(...values) : 900;
  const pad = Math.max(40, (max - min) * 0.12);
  const lo = min - pad;
  const hi = max + pad;

  /* An account made today has its whole history on a single date, so the
     window has no width and the ratio below would come out 0/0. A day of
     span puts that one sample at the left edge instead of at NaN. */
  const span = Math.max(t1 - t0, DAY_MS);
  const x = (t: number) => PAD.left + ((t - t0) / span) * PW;
  const y = (v: number) => PAD.top + (1 - (v - lo) / (hi - lo)) * PH;

  function readHover(e: React.MouseEvent<SVGRectElement>) {
    const box = e.currentTarget.getBoundingClientRect();
    const f = (e.clientX - box.left) / box.width;
    setHover(t0 + Math.min(1, Math.max(0, f)) * span);
  }

  // the readout only appears on hover: at rest the tiles above already carry
  // each current score, so repeating them here would just be noise
  const readout =
    hover === null
      ? []
      : series
          .map(({ lang, points }) => ({ lang, v: valueAt(points, hover) }))
          .filter((r) => r.v !== null);

  return (
    <div className="chart">
      <div className="chart-top">
        <div className="chart-ranges" role="group" aria-label="Graph range">
          {RANGES.map((r) => (
            <button
              key={r}
              type="button"
              className={r === range ? 'chart-range is-active' : 'chart-range'}
              onClick={() => setRange(r)}
            >
              {r}
            </button>
          ))}
        </div>
        {hover !== null && (
          <div className="chart-readout">
            <span className="chart-readout-date">{formatDate(hover)}</span>
            {readout.map(({ lang, v }) => (
              <span className="chart-readout-item" key={lang.id}>
                <span className="chart-readout-swatch" style={{ backgroundColor: lang.color }} />
                {v}
              </span>
            ))}
          </div>
        )}
      </div>

      <svg
        className="chart-svg"
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label="Score over time, one line per language"
        onMouseLeave={() => setHover(null)}
      >
        {scoreTicks(lo, hi).map((v) => (
          <g key={v}>
            <line className="chart-grid" x1={PAD.left} x2={PAD.left + PW} y1={y(v)} y2={y(v)} />
            <text className="chart-axis" x={PAD.left + PW + 8} y={y(v) + 4}>
              {v}
            </text>
          </g>
        ))}

        {dateTicks(t0, t1).map((tick) => (
          <text key={tick.t} className="chart-axis chart-axis--x" x={x(tick.t)} y={H - 8}>
            {tick.label}
          </text>
        ))}

        {series.map(({ lang, points }) => (
          <g key={lang.id}>
            <polyline
              className="chart-line"
              stroke={lang.color}
              points={points.map((p) => `${x(p.t)},${y(p.v)}`).join(' ')}
            />
            {/* a language with a single sample in the window has no line */}
            {points.length === 1 && (
              <circle cx={x(points[0].t)} cy={y(points[0].v)} r={3} fill={lang.color} />
            )}
          </g>
        ))}

        {hover !== null && (
          <g>
            <line className="chart-cross" x1={x(hover)} x2={x(hover)} y1={PAD.top} y2={PAD.top + PH} />
            {series.map(({ lang, points }) => {
              const v = valueAt(points, hover);
              return v === null ? null : (
                <circle key={lang.id} className="chart-dot" cx={x(hover)} cy={y(v)} r={4} fill={lang.color} />
              );
            })}
          </g>
        )}

        <rect
          x={PAD.left}
          y={PAD.top}
          width={PW}
          height={PH}
          fill="transparent"
          onMouseMove={readHover}
        />
      </svg>
    </div>
  );
}
