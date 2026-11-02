import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { formatMinor, nowLocalIso, parseMinor, todayIso } from "@/lib/money";
import type { AccountRow, CategoryRow, TransactionRow } from "@/lib/types";

export function TransactionsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<Partial<TransactionRow> | null>(null);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const tx = useQuery({
    queryKey: ["transactions", search],
    queryFn: () => api.transactionList<TransactionRow>({ search, limit: 300 }),
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["transactions"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const save = useMutation({
    mutationFn: (x: Record<string, unknown>) => api.transactionUpsert(x),
    onSuccess: () => { invalidate(); setEditing(null); },
  });
  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("transactions", id),
    onSuccess: invalidate,
  });

  const catName = (c?: CategoryRow) => (c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—");
  const cats = (categories.data ?? []).filter((c) => c.kind === editing?.kind);

  const submit = () => {
    if (!editing) return;
    void save.mutateAsync({
      ...editing,
      occurredAt: editing.occurredAt ?? nowLocalIso(),
      amountMinor: typeof editing.amountMinor === "number" ? editing.amountMinor : 0,
    });
  };

  const kindLabel = (k: string) => t(`ui.txKind.${k}`);

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.transactions")}</h2>
        <Button onClick={() => setEditing({ kind: "expense", occurredAt: nowLocalIso(), comment: "" })}>+ {t("ui.add")}</Button>
      </div>

      <input className="w-full rounded border border-border bg-background px-3 py-2 text-sm"
        placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />

      {editing && (
        <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
          <div className="flex gap-2">
            {(["expense", "income", "transfer"] as const).map((k) => (
              <Button key={k} size="sm" variant={editing.kind === k ? "default" : "outline"}
                onClick={() => setEditing({ ...editing, kind: k, categoryId: null })}>
                {kindLabel(k)}
              </Button>
            ))}
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.amount")}
              inputMode="decimal"
              onChange={(e) => setEditing({ ...editing, amountMinor: parseMinor(e.target.value) ?? 0 })} />
            <input className="rounded border border-border bg-background px-3 py-2 text-sm" type="date"
              defaultValue={todayIso()}
              onChange={(e) => setEditing({ ...editing, occurredAt: `${e.target.value}T${nowLocalIso().slice(11)}` })} />
            <select className="rounded border border-border bg-background px-3 py-2 text-sm"
              value={editing.accountId ?? ""} onChange={(e) => setEditing({ ...editing, accountId: e.target.value })}>
              <option value="">{t("ui.account")}</option>
              {(accounts.data ?? []).filter((a) => !a.archived).map((a) => (
                <option key={a.id} value={a.id}>{a.name}{a.last4 ? ` ·${a.last4}` : ""} ({a.currency})</option>
              ))}
            </select>
            {editing.kind === "transfer" ? (
              <select className="rounded border border-border bg-background px-3 py-2 text-sm"
                value={editing.toAccountId ?? ""} onChange={(e) => setEditing({ ...editing, toAccountId: e.target.value })}>
                <option value="">{t("ui.toAccount")}</option>
                {(accounts.data ?? []).filter((a) => !a.archived && a.id !== editing.accountId).map((a) => (
                  <option key={a.id} value={a.id}>{a.name} ({a.currency})</option>
                ))}
              </select>
            ) : (
              <select className="rounded border border-border bg-background px-3 py-2 text-sm"
                value={editing.categoryId ?? ""} onChange={(e) => setEditing({ ...editing, categoryId: e.target.value })}>
                <option value="">{t("ui.category")}</option>
                {cats.filter((c) => !c.archived).map((c) => <option key={c.id} value={c.id}>{catName(c)}</option>)}
              </select>
            )}
            <input className="rounded border border-border bg-background px-3 py-2 text-sm sm:col-span-2"
              placeholder={t("ui.comment")} value={editing.comment ?? ""}
              onChange={(e) => setEditing({ ...editing, comment: e.target.value })} />
          </div>
          <div className="flex gap-2">
            <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
            {save.isError && <span className="text-sm text-destructive">{String(save.error)}</span>}
          </div>
        </div>
      )}

      <div className="space-y-1">
        {(tx.data ?? []).map((x) => (
          <div key={x.id} className="flex items-center gap-3 rounded-xl border border-border bg-card px-3 py-2 text-sm">
            <span className={x.kind === "transfer" ? "text-muted-foreground" : x.kind === "income" ? "text-income" : "text-expense"}>
              {x.kind === "income" ? "+" : x.kind === "expense" ? "−" : "⇄"}
            </span>
            <span className="font-medium">
              {formatMinor(x.amountMinor, x.aCurrency)}
              {x.kind === "transfer" && x.toAmountMinor ? ` → ${formatMinor(x.toAmountMinor, x.a2Currency)}` : ""}
            </span>
            <span className="text-muted-foreground">
              {x.kind === "transfer" ? `${x.aName} → ${x.a2Name}` : `${x.aName} · ${catName(categories.data?.find((c) => c.id === x.categoryId))}`}
            </span>
            {x.comment && <span className="truncate text-xs text-muted-foreground">{x.comment}</span>}
            <span className="ml-auto whitespace-nowrap text-xs text-muted-foreground">
              {x.occurredAt.slice(0, 10)} {x.uName ? `· ${x.uName}` : ""} {x.source === "bot" ? "· 🤖" : ""}
            </span>
            <div className="flex gap-1">
              <Button size="sm" variant="ghost" onClick={() => setEditing(x)}>{t("ui.edit")}</Button>
              <Button size="sm" variant="ghost" onClick={() => del.mutate(x.id)}>{t("ui.delete")}</Button>
            </div>
          </div>
        ))}
        {tx.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
