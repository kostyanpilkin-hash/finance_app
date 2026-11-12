import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { convertMinorLocal, formatMinor, nowLocalIso, parseMinor, todayIso, type StoredRate } from "@/lib/money";
import { useEnsureRates } from "@/lib/useDisplayMoney";
import { cn } from "@/lib/utils";
import type { AccountRow, CategoryRow, TransactionRow } from "@/lib/types";

const ENTRY_CURRENCIES = ["BYN", "USD", "EUR", "RUB"];
/** Receipt photos are capped so a huge file can't stall the app. */
const MAX_PHOTO_MB = 15;
const MAX_PHOTO_BYTES = MAX_PHOTO_MB * 1024 * 1024;

const inputCls = "rounded border bg-background px-3 py-2 text-sm";

type Missing = "amount" | "account" | "toAccount" | "category";

/** Transaction create/edit form. `initial` seeds the state; `onDone(saved)` fires on save or cancel. */
export function TxForm({ initial, onDone }: { initial: Partial<TransactionRow>; onDone: (saved: boolean) => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [editing, setEditing] = useState<Partial<TransactionRow>>(initial);
  const [photo, setPhoto] = useState<File | null>(null);
  const [photoErr, setPhotoErr] = useState(false);
  const [accKey, setAccKey] = useState(0);
  const [tried, setTried] = useState(false);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const rates = useQuery({ queryKey: ["rates"], queryFn: () => api.ratesLatest<StoredRate>() });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["transactions"] });
    qc.invalidateQueries({ queryKey: ["tx-days"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
    qc.invalidateQueries({ queryKey: ["attachments"] });
  };
  const save = useMutation({
    mutationFn: async (x: Record<string, unknown>) => {
      const row = await api.transactionUpsert<TransactionRow>(x);
      if (photo) {
        const bytes = new Uint8Array(await photo.arrayBuffer());
        await api.attachmentAdd(row.id, photo.name, photo.type || "image/jpeg", bytes);
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

  // Lazily fetch NBRB rates the suggestion needs, then convert locally.
  useEnsureRates([editing?.origCurrency, accCurrency]);
  const suggestion =
    foreign && editing?.origAmountMinor && editing?.origCurrency && accCurrency
      ? convertMinorLocal(editing.origAmountMinor, editing.origCurrency, accCurrency, rates.data ?? [])
      : null;

  // The amount the backend would persist (same rule as submit()).
  const effectiveAmount = foreign
    ? editing?.amountMinor
    : editing?.origCurrency
      ? editing?.origAmountMinor ?? editing?.amountMinor
      : editing?.amountMinor;
  const missing = new Set<Missing>();
  if (editing) {
    if (!(effectiveAmount != null && effectiveAmount > 0)) missing.add("amount");
    if (!editing.accountId) missing.add("account");
    if (editing.kind === "transfer") {
      if (!editing.toAccountId || editing.toAccountId === editing.accountId) missing.add("toAccount");
    } else if (!editing.categoryId) {
      missing.add("category");
    }
  }
  const show = tried && missing.size > 0;
  const firstMissing: Missing | undefined = show ? (["amount", "account", "category", "toAccount"] as const).find((k) => missing.has(k)) : undefined;

  const backendErr = (e: unknown): string => {
    const m = String(e instanceof Error ? e.message : e);
    if (/same account/i.test(m)) return t("ui.err.sameAccount");
    if (/category/i.test(m)) return t("ui.err.category");
    if (/account/i.test(m)) return t("ui.err.account");
    if (/amount|original/i.test(m)) return t("ui.err.amount");
    if (/attachment|photo/i.test(m)) return t("ui.err.photoSize", { mb: MAX_PHOTO_MB });
    return t("ui.err.generic");
  };

  const submit = () => {
    if (!editing) return;
    setTried(true);
    if (missing.size > 0) return;
    const amount = effectiveAmount;
    void save.mutateAsync({
      ...editing,
      occurredAt: editing.occurredAt ?? nowLocalIso(),
      amountMinor: typeof amount === "number" ? amount : 0,
      origCurrency: foreign ? editing.origCurrency : null,
      origAmountMinor: foreign ? editing.origAmountMinor : null,
    });
  };

  const bad = (f: Missing) => (show && missing.has(f) ? "border-destructive" : "border-border");

  return (
    <form
      className="space-y-3 rounded-2xl border border-primary/40 bg-card p-4"
      onSubmit={(e) => { e.preventDefault(); submit(); }}
      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); onDone(false); } }}
    >
      <div className="flex gap-2">
        {(["expense", "income", "transfer"] as const).map((k) => (
          <Button key={k} type="button" size="sm" variant={editing.kind === k ? "default" : "outline"}
            onClick={() => setEditing({ ...editing, kind: k, categoryId: null })}>
            {t(`ui.txKind.${k}`)}
          </Button>
        ))}
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="flex gap-2">
          <input key={`amt-${foreign}-${editing.origCurrency ?? ""}`}
            className={cn("min-w-0 flex-1", inputCls, bad("amount"))}
            placeholder={t("ui.amount")} inputMode="decimal" aria-invalid={show && missing.has("amount")}
            defaultValue={(() => {
              const v = foreign ? editing.origAmountMinor : editing.amountMinor;
              return v ? formatMinor(v).replace(/\u00a0/g, "") : "";
            })()}
            onChange={(e) => {
              const v = parseMinor(e.target.value) ?? 0;
              setEditing(foreign ? { ...editing, origAmountMinor: v } : { ...editing, amountMinor: v });
            }} />
          {editing.kind !== "transfer" && accCurrency && (
            <select className={cn(inputCls, "px-2 border-border")} aria-label={t("ui.currency")}
              value={foreign ? editing.origCurrency ?? accCurrency : accCurrency} onChange={(e) => setCurrency(e.target.value)}>
              {entryCurrencies.map((c) => <option key={c} value={c}>{c}</option>)}
            </select>
          )}
        </div>
        <input className={cn(inputCls, "border-border")} type="date" defaultValue={opDate}
          onChange={(e) => setEditing({ ...editing, occurredAt: `${e.target.value}T${editing.occurredAt?.slice(11, 19) || nowLocalIso().slice(11)}` })} />
        <select className={cn(inputCls, bad("account"))} aria-invalid={show && missing.has("account")}
          value={editing.accountId ?? ""} onChange={(e) => setEditing({ ...editing, accountId: e.target.value })}>
          <option value="">{t("ui.account")}</option>
          {(accounts.data ?? []).filter((a) => !a.archived).map((a) => (
            <option key={a.id} value={a.id}>{a.name}{a.last4 ? ` ·${a.last4}` : ""} ({a.currency})</option>
          ))}
        </select>
        {foreign && (
          <div className="flex flex-wrap items-center gap-2 sm:col-span-2">
            <span className="text-sm text-muted-foreground">{t("ui.accountAmount")} ({accCurrency})</span>
            <input key={`acc-${editing.origCurrency}-${accKey}`} className={cn("w-36", inputCls, bad("amount"))}
              inputMode="decimal" placeholder="0,00"
              defaultValue={editing.amountMinor ? formatMinor(editing.amountMinor).replace(/\u00a0/g, "") : ""}
              onChange={(e) => setEditing({ ...editing, amountMinor: parseMinor(e.target.value) ?? 0 })} />
            {suggestion !== null && (
              <Button type="button" size="sm" variant="outline" onClick={() => { setEditing({ ...editing, amountMinor: suggestion }); setAccKey((k) => k + 1); }}>
                ≈ {formatMinor(suggestion, accCurrency)} · {t("ui.byRate")}
              </Button>
            )}
          </div>
        )}
        {editing.kind === "transfer" ? (
          <select className={cn(inputCls, bad("toAccount"))} aria-invalid={show && missing.has("toAccount")}
            value={editing.toAccountId ?? ""} onChange={(e) => setEditing({ ...editing, toAccountId: e.target.value })}>
            <option value="">{t("ui.toAccount")}</option>
            {(accounts.data ?? []).filter((a) => !a.archived && a.id !== editing.accountId).map((a) => (
              <option key={a.id} value={a.id}>{a.name} ({a.currency})</option>
            ))}
          </select>
        ) : (
          <select className={cn(inputCls, bad("category"))} aria-invalid={show && missing.has("category")}
            value={editing.categoryId ?? ""} onChange={(e) => setEditing({ ...editing, categoryId: e.target.value })}>
            <option value="">{t("ui.category")}</option>
            {cats.filter((c) => !c.archived).map((c) => <option key={c.id} value={c.id}>{catName(c)}</option>)}
          </select>
        )}
        <input className={cn(inputCls, "border-border sm:col-span-2")}
          placeholder={t("ui.comment")} value={editing.comment ?? ""}
          onChange={(e) => setEditing({ ...editing, comment: e.target.value })} />
        <label className="flex cursor-pointer items-center gap-2 text-sm text-muted-foreground sm:col-span-2">
          <input type="file" accept="image/*" className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0] ?? null;
              if (f && f.size > MAX_PHOTO_BYTES) {
                setPhoto(null);
                setPhotoErr(true);
              } else {
                setPhoto(f);
                setPhotoErr(false);
              }
            }} />
          📎 {photo ? photo.name : t("ui.photo")}
          {photoErr && <span className="text-destructive">{t("ui.err.photoSize", { mb: MAX_PHOTO_MB })}</span>}
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" type="submit" disabled={save.isPending}>{t("ui.save")}</Button>
        <Button size="sm" type="button" variant="outline" onClick={() => onDone(false)}>{t("ui.cancel")}</Button>
        {firstMissing && (
          <span className="text-sm text-destructive">
            {t(`ui.err.${firstMissing === "toAccount" && editing.toAccountId === editing.accountId && editing.accountId ? "sameAccount" : firstMissing}`)}
          </span>
        )}
        {save.isError && <span className="text-sm text-destructive">{backendErr(save.error)}</span>}
      </div>
    </form>
  );
}
