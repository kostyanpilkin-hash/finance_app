import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery } from "@tanstack/react-query";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { formatMinor, formatPeriodHuman, isoOf, todayIso } from "@/lib/money";
import { qk } from "@/lib/queryKeys";
import { useCategories } from "@/lib/queries";
import { useDisplayMoney } from "@/lib/useDisplayMoney";
import { cn } from "@/lib/utils";
import type { TransactionRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Field } from "@/components/ui/field";

function firstOfMonth() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}

type Preset = "thisMonth" | "prevMonth" | "quarter" | "year";

function presetRange(p: Preset): { from: string; to: string } {
  const now = new Date();
  if (p === "prevMonth") {
    const a = new Date(now.getFullYear(), now.getMonth() - 1, 1);
    const b = new Date(now.getFullYear(), now.getMonth(), 0);
    return { from: isoOf(a), to: isoOf(b) };
  }
  if (p === "quarter") {
    const q = Math.floor(now.getMonth() / 3) * 3;
    return { from: isoOf(new Date(now.getFullYear(), q, 1)), to: todayIso() };
  }
  if (p === "year") return { from: `${now.getFullYear()}-01-01`, to: todayIso() };
  return { from: firstOfMonth(), to: todayIso() };
}

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");

export function ReportsPage() {
  const { t } = useTranslation();
  const [from, setFrom] = useState(firstOfMonth());
  const [to, setTo] = useState(todayIso());

  const categories = useCategories();
  const { displayCurrency } = useDisplayMoney();
  const tx = useQuery({
    queryKey: qk.txList("report", from, to),
    queryFn: () => api.transactionList<TransactionRow>({ from, to, limit: 10000 }),
  });
  const sum = useQuery({
    queryKey: qk.analytics("summary", from, to, displayCurrency),
    queryFn: () => api.analyticsSummary(from, to, displayCurrency),
  });

  const xlsx = useMutation({ mutationFn: () => api.exportXlsx(from, to) });

  const catName = (x: TransactionRow) => {
    const c = categories.data?.find((c) => c.id === x.categoryId);
    return c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—";
  };

  const printPdf = () => {
    const rows = (tx.data ?? []).map((x) => {
      const sign = x.kind === "income" ? "+" : x.kind === "expense" ? "−" : "⇄";
      const cat = x.kind === "transfer" ? `${x.aName} → ${x.a2Name}` : `${x.aName} · ${catName(x)}`;
      return `<tr><td>${esc(x.occurredAt.slice(0, 10))}</td><td>${sign} ${formatMinor(x.amountMinor, x.aCurrency)}</td><td>${esc(cat)}</td><td>${esc(x.comment)}</td><td>${esc(x.uName ?? "")}</td><td>${esc(x.suName ?? "")}</td></tr>`;
    }).join("");
    const html = `<!doctype html><html><head><meta charset="utf-8"><title>${t("nav.reports")}</title>
      <style>body{font-family:system-ui;padding:24px}h1{font-size:20px}table{width:100%;border-collapse:collapse;font-size:12px}td,th{border-bottom:1px solid #ddd;padding:4px;text-align:left}</style>
      </head><body><h1>${t("app.title")} — ${t("nav.reports")} ${esc(formatPeriodHuman(from, to))}</h1>
      <table><tr><th>${t("ui.period.day")}</th><th>${t("ui.amount")}</th><th>${t("ui.category")}</th><th>${t("ui.comment")}</th><th>${t("ui.field.createdByUserId")}</th><th>${t("ui.whoseOp")}</th></tr>${rows}</table>
      </body></html>`;
    const url = URL.createObjectURL(new Blob([html], { type: "text/html" }));
    const iframe = document.createElement("iframe");
    iframe.style.display = "none";
    const cleanup = () => {
      URL.revokeObjectURL(url);
      iframe.remove();
    };
    iframe.onload = () => {
      const w = iframe.contentWindow;
      if (!w) return cleanup();
      w.onafterprint = cleanup;
      w.print();
      window.setTimeout(cleanup, 60_000);
    };
    iframe.src = url;
    document.body.appendChild(iframe);
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.reports")}</h2>

      <div className="rounded-2xl border border-border bg-card p-4">
        <div className="flex flex-wrap items-end gap-3">
          <Field label={t("ui.dateFrom")} className="w-44">
            <Input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
          </Field>
          <Field label={t("ui.dateTo")} className="w-44">
            <Input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
          </Field>
          <div className="flex flex-wrap gap-1.5 pb-0.5">
            {(["thisMonth", "prevMonth", "quarter", "year"] as const).map((p) => (
              <button
                key={p}
                type="button"
                className={cn(
                  "rounded-full border px-3 py-1.5 text-xs font-medium transition-colors",
                  "border-border text-muted-foreground hover:border-ring/50 hover:text-foreground",
                )}
                onClick={() => { const r = presetRange(p); setFrom(r.from); setTo(r.to); }}
              >
                {t(`ui.preset.${p}`)}
              </button>
            ))}
          </div>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-1 border-t border-border/60 pt-3 text-sm">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            {formatPeriodHuman(from, to)}
          </span>
          <span className="text-expense">−{formatMinor(sum.data?.expense ?? 0, displayCurrency)}</span>
          <span className="text-income">+{formatMinor(sum.data?.income ?? 0, displayCurrency)}</span>
          <span className={cn("font-semibold", (sum.data?.income ?? 0) - (sum.data?.expense ?? 0) > 0 ? "text-income" : (sum.data?.income ?? 0) - (sum.data?.expense ?? 0) < 0 ? "text-expense" : "text-muted-foreground")}>
            {(sum.data?.income ?? 0) - (sum.data?.expense ?? 0) > 0 ? "+" : ""}{formatMinor((sum.data?.income ?? 0) - (sum.data?.expense ?? 0), displayCurrency)}
          </span>
          <span className="text-muted-foreground">{t("ui.opsCount")}: {tx.data?.length ?? "…"}</span>
        </div>

        <div className="mt-3 flex gap-3">
          <Button onClick={() => xlsx.mutate()} disabled={xlsx.isPending}>
            {xlsx.isPending ? "…" : t("ui.exportXlsx")}
          </Button>
          <Button variant="outline" onClick={printPdf}>{t("ui.exportPdf")}</Button>
        </div>
      </div>
      {xlsx.data && (
        <div className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 text-sm">
          <span className="truncate">{xlsx.data}</span>
          <Button size="sm" variant="outline" onClick={() => void revealItemInDir(xlsx.data!)}>{t("ui.openFolder")}</Button>
        </div>
      )}
      {xlsx.isError && <p className="text-sm text-destructive">{String(xlsx.error)}</p>}
      <p className="text-xs text-muted-foreground">{t("ui.reportsHint")}</p>
    </div>
  );
}
