import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { formatMinor, nowLocalIso, parseMinor, todayIso } from "@/lib/money";
import type { AccountRow, CategoryRow, TransactionRow } from "@/lib/types";

const ENTRY_CURRENCIES = ["BYN", "USD", "EUR", "RUB"];

/** Transaction create/edit form. `initial` seeds the state; `onDone(saved)` fires on save or cancel. */
export function TxForm({ initial, onDone }: { initial: Partial<TransactionRow>; onDone: (saved: boolean) => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [editing, setEditing] = useState<Partial<TransactionRow>>(initial);
  const [photo, setPhoto] = useState<File | null>(null);
  const [accKey, setAccKey] = useState(0);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["transactions"] });
    qc.invalidateQueries({ queryKey: ["tx-days"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const save = useMutation({
    mutationFn: async (x: Record<string, unknown>) => {
      const row = await api.transactionUpsert<TransactionRow>(x);
      if (photo) {
        const buf = new Uint8Array(await photo.arrayBuffer());
        let bin = "";
        buf.forEach((b) => (bin += String.fromCharCode(b)));
        await api.attachmentAdd(row.id, photo.name, photo.type || "image/jpeg", btoa(bin));
      }
      return row;
    },
    onSuccess: () => { invalidate(); onDone(true); },
  });

  const catName = (c?: CategoryRow) => (c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—");
  const cats = (categories.data ?? []).filter((c) => c.kind === editing?.kind);
  const accCurrency = (accounts.data ?? []).find((a) => a.id === editing?.accountId)?.currency;
  const foreign = editing?.kind !== "transfer" && !!editing?.origCurrency && editing.origCurrency !== accCurrency;
  const entryCurrencies = [...new Set([accCurrency, ...ENTRY_CURRENCIES].filter(Boolean) as string[])];
  const setCurrency = (cur: string) => {
    if (!editing) return;
    if (cur === accCurrency) {
      setEditing({ ...editing, origCurrency: null, origAmountMinor: null, amountMinor: editing.origAmountMinor ?? editing.amountMinor });
    } else {
      setEditing({ ...editing, origCurrency: cur, origAmountMinor: editing.origAmountMinor ?? editing.amountMinor, amountMinor: 0 });
    }
  };
  const opDate = (editing?.occurredAt ?? "").slice(0, 10) || todayIso();
  const conv = useQuery({
    queryKey: ["convertAt", editing?.origAmountMinor, editing?.origCurrency, accCurrency, opDate],
    queryFn: () => api.convertAt(editing!.origAmountMinor!, editing!.origCurrency!, accCurrency!, opDate),
    enabled: foreign && !!editing?.origAmountMinor && !!accCurrency,
  });
  const suggestion = conv.data ?? null;

  const submit = () => {
    if (!editing) return;
    const isForeign = editing.kind !== "transfer" && !!editing.origCurrency && editing.origCurrency !== accCurrency;
    const amount = isForeign ? editing.amountMinor : editing.origCurrency ? editing.origAmountMinor ?? editing.amountMinor : editing.amountMinor;
    void save.mutateAsync({
      ...editing,
      occurredAt: editing.occurredAt ?? nowLocalIso(),
      amountMinor: typeof amount === "number" ? amount : 0,
      origCurrency: isForeign ? editing.origCurrency : null,
      origAmountMinor: isForeign ? editing.origAmountMinor : null,
    });
  };

  return (
    <div className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4">
      <div className="flex gap-2">
        {(["expense", "income", "transfer"] as const).map((k) => (
          <Button key={k} size="sm" variant={editing.kind === k ? "default" : "outline"}
            onClick={() => setEditing({ ...editing, kind: k, categoryId: null })}>
            {t(`ui.txKind.${k}`)}
          </Button>
        ))}
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="flex gap-2">
          <input className="min-w-0 flex-1 rounded border border-border bg-background px-3 py-2 text-sm" placeholder={t("ui.amount")}
            inputMode="decimal"
            onChange={(e) => {
              const v = parseMinor(e.target.value) ?? 0;
              setEditing(foreign ? { ...editing, origAmountMinor: v } : { ...editing, amountMinor: v });
            }} />
          {editing.kind !== "transfer" && accCurrency && (
            <select className="rounded border border-border bg-background px-2 py-2 text-sm" aria-label={t("ui.currency")}
              value={foreign ? editing.origCurrency ?? accCurrency : accCurrency} onChange={(e) => setCurrency(e.target.value)}>
              {entryCurrencies.map((c) => <option key={c} value={c}>{c}</option>)}
            </select>
          )}
        </div>
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
        {foreign && (
          <div className="flex flex-wrap items-center gap-2 sm:col-span-2">
            <span className="text-sm text-muted-foreground">{t("ui.accountAmount")} ({accCurrency})</span>
            <input key={`acc-${editing.origCurrency}-${accKey}`} className="w-36 rounded border border-border bg-background px-3 py-2 text-sm"
              inputMode="decimal" placeholder="0,00"
              defaultValue={editing.amountMinor ? formatMinor(editing.amountMinor).replace(/\u00a0/g, "") : ""}
              onChange={(e) => setEditing({ ...editing, amountMinor: parseMinor(e.target.value) ?? 0 })} />
            {suggestion !== null && (
              <Button size="sm" variant="outline" onClick={() => { setEditing({ ...editing, amountMinor: suggestion }); setAccKey((k) => k + 1); }}>
                ≈ {formatMinor(suggestion, accCurrency)} · {t("ui.byRate")}
              </Button>
            )}
          </div>
        )}
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
        <label className="flex cursor-pointer items-center gap-2 text-sm text-muted-foreground sm:col-span-2">
          <input type="file" accept="image/*" className="hidden" onChange={(e) => setPhoto(e.target.files?.[0] ?? null)} />
          📎 {photo ? photo.name : t("ui.photo")}
        </label>
      </div>
      <div className="flex gap-2">
        <Button size="sm" onClick={submit} disabled={save.isPending}>{t("ui.save")}</Button>
        <Button size="sm" variant="outline" onClick={() => onDone(false)}>{t("ui.cancel")}</Button>
        {save.isError && <span className="text-sm text-destructive">{String(save.error)}</span>}
      </div>
    </div>
  );
}
