import { useId, useMemo, useState } from "react";
import { formatMinor } from "@/lib/money";
import { shade, textOn } from "@/lib/color";
import { CategoryIcon } from "@/components/ui/category-icon";

export interface DonutItem {
  /** Source row id (e.g. a category id); null marks "no category". */
  id?: string | null;
  name: string;
  value: number;
  fill: string;
  /** Category icon id shown in the legend instead of a colour dot. */
  iconId?: string;
  /** Single letter shown on a colour circle (e.g. a person's initial). */
  initial?: string;
}

const CX = 110;
const CY = 110;
const R_OUT = 97;
const R_IN = 60;
const ACTIVE_LIFT = 6;
const GAP_DEG = 1.6;
const LABEL_MIN_FRAC = 0.07;
const OTHER_FILL = "#9ca3af";
const MIN_SHARE = 0.02;

/** Fallback hues when several slices share the same fill. */
const PALETTE = [
  "#10b981", "#f59e0b", "#3b82f6", "#ec4899", "#8b5cf6", "#14b8a6",
  "#f97316", "#84cc16", "#06b6d4", "#ef4444", "#eab308", "#a855f7",
];

interface DonutChartProps {
  data: DonutItem[];
  currency?: string;
  /** Center label while nothing is hovered. */
  totalLabel?: string;
  emptyLabel?: string;
  /** Name for the merged tail of tiny segments. */
  otherLabel?: string;
  /** Called with the slice's source items and its display name on click. */
  onSelect?: (members: DonutItem[], label: string) => void;
}

interface Slice extends DonutItem {
  /** Cumulative fraction where this slice starts on the ring. */
  start: number;
  frac: number;
  other?: boolean;
  /** Source items merged into this slice (deduped names or the tiny tail). */
  members: DonutItem[];
}

function polar(r: number, deg: number): [number, number] {
  const a = ((deg - 90) * Math.PI) / 180;
  return [CX + r * Math.cos(a), CY + r * Math.sin(a)];
}

/** Annular sector path from angle a0 to a1 (degrees, clockwise from top). */
function sectorPath(rOut: number, rIn: number, a0: number, a1: number): string {
  const [x0o, y0o] = polar(rOut, a0);
  const [x1o, y1o] = polar(rOut, a1);
  const [x1i, y1i] = polar(rIn, a1);
  const [x0i, y0i] = polar(rIn, a0);
  const large = a1 - a0 > 180 ? 1 : 0;
  return [
    `M ${x0o.toFixed(2)} ${y0o.toFixed(2)}`,
    `A ${rOut} ${rOut} 0 ${large} 1 ${x1o.toFixed(2)} ${y1o.toFixed(2)}`,
    `L ${x1i.toFixed(2)} ${y1i.toFixed(2)}`,
    `A ${rIn} ${rIn} 0 ${large} 0 ${x0i.toFixed(2)} ${y0i.toFixed(2)}`,
    "Z",
  ].join(" ");
}

/** Segments below MIN_SHARE merge into "other"; duplicate fills are re-hued. */
function useSlices(data: DonutItem[]) {
  return useMemo(() => {
    const byName = new Map<string, { item: DonutItem; members: DonutItem[] }>();
    for (const d of data) {
      const g = byName.get(d.name);
      if (g) {
        g.item.value += d.value;
        g.members.push(d);
      } else {
        byName.set(d.name, { item: { ...d }, members: [d] });
      }
    }
    const groups = [...byName.values()].sort((a, b) => b.item.value - a.item.value);
    const total = groups.reduce((s, g) => s + g.item.value, 0);
    if (total <= 0) return { slices: [] as Slice[], total };
    const big = groups.filter((g) => g.item.value / total >= MIN_SHARE || groups.length <= 6);
    const small = groups.filter((g) => !big.includes(g));
    const fracs: Slice[] = big.map((g) => ({
      ...g.item,
      members: g.members,
      start: 0,
      frac: g.item.value / total,
    }));
    if (small.length > 0) {
      const v = small.reduce((s, g) => s + g.item.value, 0);
      fracs.push({
        name: "…",
        value: v,
        fill: OTHER_FILL,
        start: 0,
        frac: v / total,
        other: true,
        members: small.flatMap((g) => g.members),
      });
    }
    const used = new Set<string>();
    for (const s of fracs) {
      const key = s.fill.toLowerCase();
      if (used.has(key)) {
        const free = PALETTE.find((p) => !used.has(p));
        s.fill = free ?? s.fill;
      }
      used.add(s.fill.toLowerCase());
    }
    let acc = 0;
    const slices = fracs.map((s) => {
      const out = { ...s, start: acc };
      acc += s.frac;
      return out;
    });
    return { slices, total };
  }, [data]);
}

export function DonutChart({ data, currency, totalLabel, emptyLabel = "—", otherLabel, onSelect }: DonutChartProps) {
  const { slices, total } = useSlices(data);
  const [active, setActive] = useState<number | null>(null);
  const gid = useId().replace(/[^a-zA-Z0-9]/g, "");

  if (slices.length === 0) {
    return <div className="flex h-64 items-center justify-center text-sm text-muted-foreground">{emptyLabel}</div>;
  }

  const segs = slices.map((s, i) => {
    const full = s.frac >= 0.9999;
    const gap = full || slices.length === 1 ? 0 : GAP_DEG;
    const a0 = s.start * 360 + gap / 2;
    const a1 = Math.max(a0 + 0.2, Math.min((s.start + s.frac) * 360 - gap / 2, a0 + 359.99));
    const mid = (a0 + a1) / 2;
    const rad = (mid * Math.PI) / 180;
    const lift = active === i ? ACTIVE_LIFT : 0;
    return {
      ...s,
      a0,
      a1,
      mid,
      dx: Math.sin(rad) * 5,
      dy: -Math.cos(rad) * 5,
      rOut: R_OUT + (active === i ? lift : 0),
    };
  });

  const center = active !== null ? segs[active] : null;
  const pct = (v: number) => `${Math.round((v / total) * 100)}%`;
  const labelOf = (s: (typeof segs)[number]) => (s.other && otherLabel ? otherLabel : s.name);

  return (
    <div className="flex flex-col items-center gap-4 sm:flex-row">
      <div className="relative h-52 w-52 shrink-0">
        <svg viewBox="0 0 220 220" className="h-full w-full overflow-visible">
          <defs>
            {segs.map((s, i) => (
              <linearGradient key={i} id={`${gid}${i}`} x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stopColor={shade(s.fill, 16)} />
                <stop offset="100%" stopColor={shade(s.fill, -12)} />
              </linearGradient>
            ))}
          </defs>
          <g
            style={{
              transformBox: "view-box",
              transformOrigin: "center",
              animation: "donutIn .55s cubic-bezier(.2,.8,.3,1.05) both",
            }}
          >
            {segs.map((s, i) => (
              <g
                key={i}
                className="cursor-pointer"
                style={{
                  transform: `translate(${s.dx}px, ${s.dy}px)`,
                  transition: "transform .25s ease, opacity .25s ease",
                  opacity: active === null || active === i ? 1 : 0.32,
                }}
                onMouseEnter={() => setActive(i)}
                onMouseLeave={() => setActive(null)}
                onClick={() => onSelect?.(s.members, labelOf(s))}
              >
                <path d={sectorPath(s.rOut, R_IN, s.a0, s.a1)} fill={`url(#${gid}${i})`} />
                {s.frac >= LABEL_MIN_FRAC && (
                  <text
                    x={polar((s.rOut + R_IN) / 2, s.mid)[0]}
                    y={polar((s.rOut + R_IN) / 2, s.mid)[1]}
                    textAnchor="middle"
                    dominantBaseline="central"
                    fontSize={13}
                    fontWeight={700}
                    fill={textOn(s.fill)}
                    className="pointer-events-none select-none"
                  >
                    {pct(s.value)}
                  </text>
                )}
              </g>
            ))}
          </g>
        </svg>
        <style>{`@keyframes donutIn { from { transform: rotate(-70deg) scale(.55); opacity: 0; } }`}</style>
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center text-center">
          <div className="max-w-[90px] truncate text-xs font-medium text-muted-foreground">
            {center ? labelOf(center) : totalLabel ?? ""}
          </div>
          <div className="font-display text-lg font-semibold leading-tight">
            {formatMinor(center ? center.value : total, currency)}
          </div>
          {center && <div className="text-xs text-muted-foreground">{pct(center.value)}</div>}
        </div>
      </div>

      <div className="w-full min-w-0 flex-1 space-y-0.5">
        {segs.map((s, i) => (
          <button
            key={i}
            type="button"
            className="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-2 text-left text-sm transition-colors hover:bg-muted/60"
            onMouseEnter={() => setActive(i)}
            onMouseLeave={() => setActive(null)}
            onClick={() => onSelect?.(s.members, labelOf(s))}
          >
            {s.iconId ? (
              <CategoryIcon icon={s.iconId} color={s.fill} size={20} />
            ) : s.initial ? (
              <span
                className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-xs font-bold text-white"
                style={{ background: `linear-gradient(135deg, ${shade(s.fill, 16)}, ${shade(s.fill, -12)})` }}
              >
                {s.initial}
              </span>
            ) : (
              <span
                className="h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ background: `linear-gradient(135deg, ${shade(s.fill, 16)}, ${shade(s.fill, -12)})` }}
              />
            )}
            <span className="min-w-0 flex-1 truncate text-muted-foreground">{labelOf(s)}</span>
            <span className="shrink-0 font-medium">{formatMinor(s.value, currency)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
