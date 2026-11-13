import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery } from "@tanstack/react-query";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { formatMinor, todayIso } from "@/lib/money";
import type { CategoryRow, TransactionRow } from "@/lib/types";
import { Input } from "@/components/ui/input";

function firstOfMonth() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");

export function ReportsPage() {
  const { t } = useTranslation();
  const [from, setFrom] = useState(firstOfMonth());
  const [to, setTo] = useState(todayIso());

  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const tx = useQuery({
    queryKey: ["tx", "report", from, to],
    queryFn: () => api.transactionList<TransactionRow>({ from, to, limit: 10000 }),
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
      </head><body><h1>${t("app.title")} — ${t("nav.reports")} ${esc(from)} — ${esc(to)}</h1>
      <table><tr><th>${t("ui.period.day")}</th><th>${t("ui.amount")}</th><th>${t("ui.category")}</th><th>${t("ui.comment")}</th><th>${t("ui.field.createdByUserId")}</th><th>${t("ui.whoseOp")}</th></tr>${rows}</table>
      </body></html>`;
    const iframe = document.createElement("iframe");
    iframe.style.display = "none";
    iframe.src = URL.createObjectURL(new Blob([html], { type: "text/html" }));
    iframe.onload = () => iframe.contentWindow?.print();
    document.body.appendChild(iframe);
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.reports")}</h2>
      <div className="flex flex-wrap items-center gap-2">
        <Input type="date" 
          value={from} onChange={(e) => setFrom(e.target.value)} />
        <Input type="date" 
          value={to} onChange={(e) => setTo(e.target.value)} />
      </div>
      <div className="flex gap-3">
        <Button onClick={() => xlsx.mutate()} disabled={xlsx.isPending}>
          {xlsx.isPending ? "…" : t("ui.exportXlsx")}
        </Button>
        <Button variant="outline" onClick={printPdf}>{t("ui.exportPdf")}</Button>
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
