import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { BarChart, Bar, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } from "recharts";
import { api } from "@/lib/api";
import { formatMinor, todayIso } from "@/lib/money";
import type { TransactionRow } from "@/lib/types";

type Kind = "day" | "week" | "month" | "custom";

function iso(d: Date) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function range(kind: Kind, anchor: Date, custom: { from: string; to: string }) {
  if (kind === "custom") return { from: custom.from, to: custom.to };
  const d = new Date(anchor);
  if (kind === "day") return { from: iso(d), to: iso(d) };
  if (kind === "week") {
    const wd = (d.getDay() + 6) % 7;
    const mon = new Date(d); mon.setDate(d.getDate() - wd);
    const sun = new Date(mon); sun.setDate(mon.getDate() + 6);
    return { from: iso(mon), to: iso(sun) };
  }
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const last = new Date(d.getFullYear(), d.getMonth() + 1, 0);
  return { from: iso(first), to: iso(last) };
}

function prevRange(kind: Kind, r: { from: string; to: string }) {
  const a = new Date(r.from), b = new Date(r.to);
  const shift = (d: Date, days: number) => { const x = new Date(d); x.setDate(x.getDate() - days); return x; };
  if (kind === "day") return { from: iso(shift(a, 1)), to: iso(shift(a, 1)) };
  if (kind === "week") return { from: iso(shift(a, 7)), to: iso(shift(b, 7)) };
  if (kind === "month") {
    const first = new Date(a.getFullYear(), a.getMonth() - 1, 1);
    const last = new Date(a.getFullYear(), a.getMonth(), 0);
    return { from: iso(first), to: iso(last) };
  }
  const len = Math.round((b.getTime() - a.getTime()) / 86400000) + 1;
  return { from: iso(shift(a, len)), to: iso(shift(b, len)) };
}

const PALETTE = { expense: "var(--expense,#dc2626)", income: "var(--income,#16a34a)" };

export function AnalyticsPage() {
  const { t } = useTranslation();
  const [kind, setKind] = useState<Kind>("month");
  const [custom, setCustom] = useState({ from: todayIso(), to: todayIso() });
  const r = range(kind, new Date(), custom);
  const pr = prevRange(kind, r);

  const cur = useQuery({
    queryKey: ["tx", "analytics", r.from, r.to],
    queryFn: () => api.transactionList<TransactionRow>({ from: r.from, to: r.to, limit: 5000 }),
  });
  const prev = useQuery({
    queryKey: ["tx", "analytics", pr.from, pr.to],
    queryFn: () => api.transactionList<TransactionRow>({ from: pr.from, to: pr.to, limit: 5000 }),
  });

  const sum = (rows: TransactionRow[], k: string) =>
    rows.filter((x) => x.kind === k).reduce((s, x) => s + x.amountMinor, 0);

  const chart = useMemo(() => {
    const buckets = new Map<string, { label: string; expense: number; income: number }>();
    for (const x of cur.data ?? []) {
      if (x.kind === "transfer") continue;
      const day = x.occurredAt.slice(0, 10);
      const b = buckets.get(day) ?? { label: day.slice(5), expense: 0, income: 0 };
      b[x.kind as "expense" | "income"] += x.amountMinor;
      buckets.set(day, b);
    }
    return [...buckets.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([, v]) => ({ ...v, expense: v.expense / 100, income: v.income / 100 }));
  }, [cur.data]);

  const e = sum(cur.data ?? [], "expense");
  const i = sum(cur.data ?? [], "income");
  const pe = sum(prev.data ?? [], "expense");
  const pi = sum(prev.data ?? [], "income");
  const delta = (a: number, b: number) => (b === 0 ? (a === 0 ? 0 : 100) : Math.round(((a - b) / b) * 100));

  const stat = (label: string, v: number, pv: number, cls: string) => (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`text-xl font-semibold ${cls}`}>{formatMinor(v)}</div>
      <div className="text-xs text-muted-foreground">
        {t("ui.vsPrev")}: {pv ? formatMinor(pv) : "—"} {pv || v ? `(${delta(v, pv) > 0 ? "+" : ""}${delta(v, pv)}%)` : ""}
      </div>
    </div>
  );

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.analytics")}</h2>

      <div className="flex flex-wrap items-center gap-2">
        {(["day", "week", "month", "custom"] as const).map((k) => (
          <button key={k} onClick={() => setKind(k)}
            className={`rounded-lg border px-3 py-1.5 text-sm ${kind === k ? "border-primary bg-primary text-primary-foreground" : "border-border bg-card"}`}>
            {t(`ui.period.${k}`)}
          </button>
        ))}
        {kind === "custom" && (
          <>
            <input type="date" className="rounded border border-border bg-background px-2 py-1.5 text-sm"
              value={custom.from} onChange={(e) => setCustom({ ...custom, from: e.target.value })} />
            <input type="date" className="rounded border border-border bg-background px-2 py-1.5 text-sm"
              value={custom.to} onChange={(e) => setCustom({ ...custom, to: e.target.value })} />
          </>
        )}
        <span className="text-sm text-muted-foreground">{r.from} — {r.to}</span>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        {stat(t("ui.expenses"), e, pe, "text-expense")}
        {stat(t("ui.incomes"), i, pi, "text-income")}
        {stat(t("ui.net"), i - e, pi - pe, "")}
      </div>

      <div className="rounded-2xl border border-border bg-card p-4">
        <div className="mb-2 text-sm font-medium">{t("ui.byDay")}</div>
        <div className="h-72">
          <ResponsiveContainer>
            <BarChart data={chart}>
              <XAxis dataKey="label" fontSize={11} />
              <YAxis fontSize={11} />
              <Tooltip formatter={(v) => formatMinor(Math.round(Number(v) * 100))} />
              <Legend />
              <Bar dataKey="expense" name={t("ui.expenses")} fill={PALETTE.expense} radius={[3, 3, 0, 0]} />
              <Bar dataKey="income" name={t("ui.incomes")} fill={PALETTE.income} radius={[3, 3, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}
