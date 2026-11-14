import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { RichSelect } from "@/components/ui/rich-select";
import { CategoryIcon } from "@/components/ui/category-icon";
import { convertMinorLocal, formatMinor, nowLocalIso, parseMinor, todayIso, type StoredRate } from "@/lib/money";
import { ownerLabelKey } from "@/lib/owners";
import { qk, invalidateMoney } from "@/lib/queryKeys";
import { useAccounts, useCategories, useSettings, useUsers } from "@/lib/queries";
import { useEnsureRates } from "@/lib/useDisplayMoney";
import { TxAttachments } from "@/components/attachments/TxAttachments";
import type { CategoryRow, TransactionRow } from "@/lib/types";

const ENTRY_CURRENCIES = ["BYN", "USD", "EUR", "RUB"];
/** Receipt photos are capped so a huge file can't stall the app. */
const MAX_PHOTO_MB = 15;
const MAX_PHOTO_BYTES = MAX_PHOTO_MB * 1024 * 1024;

type Missing = "amount" | "account" | "toAccount" | "toAmount" | "category";

/** Transaction create/edit form. `initial` seeds the state; `onDone(saved)` fires on save or cancel. */
export function TxForm({ initial, onDone }: { initial: Partial<TransactionRow>; onDone: (saved: boolean) => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [editing, setEditing] = useState<Partial<TransactionRow>>(initial);
  const [photo, setPhoto] = useState<File | null>(null);
  const [photoErr, setPhotoErr] = useState(false);
  const [accKey, setAccKey] = useState(0);
  const [toKey, setToKey] = useState(0);
  const [tried, setTried] = useState(false);

  const accounts = useAccounts();
  const categories = useCategories();
  const users = useUsers();
  const settings = useSettings();
  const rates = useQuery({ queryKey: qk.rates, queryFn: () => api.ratesLatest<StoredRate>() });

  const invalidate = () => invalidateMoney(qc);
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
  // Every entry carries an owner: on edit it is the saved (or author) user, on
  // create it defaults to this device's current user.
  const spenderId = editing?.spentByUserId ?? editing?.createdByUserId ?? settings.data?.["current_user_id"] ?? "";
  const spenderLabel = t(ownerLabelKey(editing?.kind ?? "expense"));
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
  const toAccCurrency = (accounts.data ?? []).find((a) => a.id === editing?.toAccountId)?.currency;
  // A cross-currency transfer needs the credited amount in the destination currency.
  const crossCurrency = editing?.kind === "transfer" && !!toAccCurrency && !!accCurrency && toAccCurrency !== accCurrency;
  const opDate = (editing?.occurredAt ?? "").slice(0, 10) || todayIso();

  // Lazily fetch NBRB rates the suggestion needs, then convert locally.
  useEnsureRates([editing?.origCurrency, accCurrency, crossCurrency ? toAccCurrency : undefined]);
  const suggestion =
    foreign && editing?.origAmountMinor && editing?.origCurrency && accCurrency
      ? convertMinorLocal(editing.origAmountMinor, editing.origCurrency, accCurrency, rates.data ?? [])
      : null;
  const convSuggestion =
    crossCurrency && editing?.amountMinor && accCurrency && toAccCurrency
      ? convertMinorLocal(editing.amountMinor, accCurrency, toAccCurrency, rates.data ?? [])
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
      if (crossCurrency && !(editing.toAmountMinor != null && editing.toAmountMinor > 0)) missing.add("toAmount");
    } else if (!editing.categoryId) {
      missing.add("category");
    }
  }
  const show = tried && missing.size > 0;
  const firstMissing: Missing | undefined = show ? (["amount", "account", "category", "toAccount", "toAmount"] as const).find((k) => missing.has(k)) : undefined;

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
      toAmountMinor: crossCurrency ? (editing.toAmountMinor ?? null) : null,
      spentByUserId: spenderId || null,
    });
  };

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
        <Field label={t("ui.amount")} htmlFor="tx-amount">
          <div className="flex gap-2">
            <Input key={`amt-${foreign}-${editing.origCurrency ?? ""}`} id="tx-amount"
              className="min-w-0 flex-1"
              invalid={show && missing.has("amount")}
              placeholder="0,00" inputMode="decimal"
              defaultValue={(() => {
                const v = foreign ? editing.origAmountMinor : editing.amountMinor;
                return v ? formatMinor(v).replace(/[\u00A0\u202F]/g, "") : "";
              })()}
              onChange={(e) => {
                const v = parseMinor(e.target.value) ?? 0;
                setEditing(foreign ? { ...editing, origAmountMinor: v } : { ...editing, amountMinor: v });
              }} />
            {editing.kind !== "transfer" && accCurrency && (
              <RichSelect className="w-24 shrink-0" aria-label={t("ui.currency")}
                value={foreign ? editing.origCurrency ?? accCurrency : accCurrency} onChange={setCurrency}
                items={entryCurrencies.map((c) => ({ value: c, label: c }))} />
            )}
          </div>
        </Field>
        <Field label={t("ui.date")} htmlFor="tx-date">
          <Input id="tx-date" type="date" defaultValue={opDate}
            onChange={(e) => setEditing({ ...editing, occurredAt: `${e.target.value}T${editing.occurredAt?.slice(11, 19) || nowLocalIso().slice(11)}` })} />
        </Field>
        <Field label={t("ui.account")} htmlFor="tx-account">
          <RichSelect id="tx-account" invalid={show && missing.has("account")}
            placeholder={t("ui.account")}
            value={editing.accountId ?? ""} onChange={(v) => setEditing({ ...editing, accountId: v })}
            items={[
              { value: "", label: t("ui.account") },
              ...(accounts.data ?? []).filter((a) => !a.archived).map((a) => ({
                value: a.id, label: `${a.name}${a.last4 ? ` ·${a.last4}` : ""} (${a.currency})`, dot: a.color,
              })),
            ]} />
        </Field>
        {foreign && (
          <div className="sm:col-span-2">
            <Field label={`${t("ui.accountAmount")} (${accCurrency})`} htmlFor="tx-acc-amount">
              <div className="flex flex-wrap items-center gap-2">
                <Input key={`acc-${editing.origCurrency}-${accKey}`} id="tx-acc-amount" className="w-36"
                  invalid={show && missing.has("amount")}
                  inputMode="decimal" placeholder="0,00"
                  defaultValue={editing.amountMinor ? formatMinor(editing.amountMinor).replace(/[\u00A0\u202F]/g, "") : ""}
                  onChange={(e) => setEditing({ ...editing, amountMinor: parseMinor(e.target.value) ?? 0 })} />
                {suggestion !== null && (
                  <Button type="button" size="sm" variant="outline" onClick={() => { setEditing({ ...editing, amountMinor: suggestion }); setAccKey((k) => k + 1); }}>
                    ≈ {formatMinor(suggestion, accCurrency)} · {t("ui.byRate")}
                  </Button>
                )}
              </div>
            </Field>
          </div>
        )}
        <Field label={editing.kind === "transfer" ? t("ui.toAccount") : t("ui.category")}
          htmlFor={editing.kind === "transfer" ? "tx-to-account" : "tx-category"}>
          {editing.kind === "transfer" ? (
            <RichSelect id="tx-to-account" invalid={show && missing.has("toAccount")}
              placeholder={t("ui.toAccount")}
              value={editing.toAccountId ?? ""} onChange={(v) => setEditing({ ...editing, toAccountId: v })}
              items={[
                { value: "", label: t("ui.toAccount") },
                ...(accounts.data ?? []).filter((a) => !a.archived && a.id !== editing.accountId).map((a) => ({
                  value: a.id, label: `${a.name} (${a.currency})`, dot: a.color,
                })),
              ]} />
          ) : (
            <RichSelect id="tx-category" invalid={show && missing.has("category")}
              placeholder={t("ui.category")}
              value={editing.categoryId ?? ""} onChange={(v) => setEditing({ ...editing, categoryId: v })}
              items={[
                { value: "", label: t("ui.category") },
                ...cats.filter((c) => !c.archived).map((c) => ({
                  value: c.id, label: catName(c),
                  icon: <CategoryIcon icon={c.icon} color={c.color} size={20} />,
                })),
              ]} />
          )}
        </Field>
        {crossCurrency && (
          <Field label={`${t("ui.field.toAmountMinor")} (${toAccCurrency})`} htmlFor="tx-to-amount">
            <div className="flex flex-wrap items-center gap-2">
              <Input key={`to-${accCurrency}-${toAccCurrency}-${toKey}`} id="tx-to-amount" className="w-36"
                invalid={show && missing.has("toAmount")}
                inputMode="decimal" placeholder="0,00"
                defaultValue={editing?.toAmountMinor ? formatMinor(editing.toAmountMinor).replace(/[\u00A0\u202F]/g, "") : ""}
                onChange={(e) => setEditing({ ...editing, toAmountMinor: parseMinor(e.target.value) ?? null })} />
              {convSuggestion !== null && editing?.toAmountMinor !== convSuggestion && (
                <Button type="button" size="sm" variant="outline"
                  onClick={() => { setEditing({ ...editing, toAmountMinor: convSuggestion }); setToKey((k) => k + 1); }}>
                  ≈ {formatMinor(convSuggestion, toAccCurrency)} · {t("ui.byRate")}
                </Button>
              )}
            </div>
          </Field>
        )}
        <Field label={spenderLabel} htmlFor="tx-spent-by">
          <RichSelect id="tx-spent-by" value={spenderId}
            onChange={(v) => setEditing({ ...editing, spentByUserId: v })}
            items={(users.data ?? []).map((u) => ({ value: u.id, label: u.name, dot: u.color }))} />
        </Field>
        <Field label={t("ui.comment")} htmlFor="tx-comment" className="sm:col-span-2">
          <Input id="tx-comment" value={editing.comment ?? ""}
            onChange={(e) => setEditing({ ...editing, comment: e.target.value })} />
        </Field>
        <label className="flex min-h-9 cursor-pointer items-center gap-2 text-sm text-muted-foreground sm:col-span-2">
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
        {editing.id && (
          <div className="sm:col-span-2">
            <TxAttachments txId={editing.id} editable />
          </div>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" type="submit" loading={save.isPending}>{t("ui.save")}</Button>
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
