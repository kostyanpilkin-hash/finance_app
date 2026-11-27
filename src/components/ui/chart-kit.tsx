import type { ReactNode } from "react";
import { formatMinor } from "@/lib/money";

interface TipItem {
  name?: ReactNode;
  value?: unknown;
  color?: string;
  dataKey?: unknown;
}

/** Card-styled tooltip; values are minor units in `currency`. */
export function ChartTooltip({
  active,
  payload,
  label,
  currency,
  formatLabel,
  hint,
}: {
  active?: boolean;
  payload?: readonly TipItem[];
  label?: ReactNode;
  currency: string;
  formatLabel?: (label: ReactNode) => ReactNode;
  /** Small footer line, e.g. "click to see operations". */
  hint?: string;
}) {
  if (!active || !payload?.length) return null;
  return (
    <div className="min-w-40 rounded-xl border border-border bg-card px-3 py-2 text-xs shadow-[var(--card-shadow)]">
      <div className="mb-1 font-medium text-foreground">{formatLabel ? formatLabel(label) : label}</div>
      {payload.map((p, i) => (
        <div key={String(p.dataKey ?? i)} className="flex items-center gap-2 py-0.5">
          <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: p.color }} />
          <span className="flex-1 text-muted-foreground">{p.name}</span>
          <span className="font-medium tabular-nums text-foreground">{formatMinor(Number(p.value), currency)}</span>
        </div>
      ))}
      {hint && <div className="mt-1 border-t border-border/60 pt-1 text-[11px] text-muted-foreground">{hint}</div>}
    </div>
  );
}

/** Compact legend for a card header (replaces recharts' default legend). */
export function ChartLegend({ items }: { items: { color: string; label: string; line?: boolean }[] }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {items.map((it) => (
        <span key={it.label} className="inline-flex items-center gap-1.5">
          <span
            className={it.line ? "h-0.5 w-3 rounded-full" : "h-2 w-2 rounded-full"}
            style={{ background: it.color }}
          />
          {it.label}
        </span>
      ))}
    </div>
  );
}

/** Card heading row: title left, legend (or any extra) right. */
export function ChartHead({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
      <div className="text-sm font-medium">{title}</div>
      {children}
    </div>
  );
}
