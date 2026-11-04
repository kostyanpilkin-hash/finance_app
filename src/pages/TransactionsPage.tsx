import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { CategoryIcon } from "@/components/ui/category-icon";
import { formatMinor, nowLocalIso, parseMinor, todayIso } from "@/lib/money";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AccountRow, CategoryRow, TransactionRow } from "@/lib/types";

const ENTRY_CURRENCIES = ["BYN", "USD", "EUR", "RUB"];

interface AttachmentRow { id: string; transactionId: string; sha256: string; fileName: string }

function Thumb({ txId }: { txId: string }) {
  const atts = useQuery({ queryKey: ["attachments"], queryFn: () => api.entityList<AttachmentRow>("attachments") });
  const att = atts.data?.find((a) => a.transactionId === txId);
  const path = useQuery({
    queryKey: ["attachment-path", att?.sha256],
    queryFn: () => api.attachmentPath(att!.sha256),
    enabled: !!att,
  });
  if (!path.data) return null;
  return <img src={convertFileSrc(path.data)} alt="" className="h-8 w-8 rounded object-cover" />;
}

export function TransactionsPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [accountId, setAccountId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [editing, setEditing] = useState<Partial<TransactionRow> | null>(null);
  const [photo, setPhoto] = useState<File | null>(null);
  const [accKey, setAccKey] = useState(0);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });
  const tx = useQuery({
    queryKey: ["transactions", search, accountId, categoryId],
    queryFn: () => api.transactionList<TransactionRow>({ search, accountId: accountId || undefined, categoryId: categoryId || undefined, limit: 500 }),
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["transactions"] });
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
    onSuccess: () => { invalidate(); setEditing(null); setPhoto(null); },
  });
  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("transactions", id),
    onSuccess: invalidate,
  });

  const catName = (c?: CategoryRow) => (c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—");
  const cats = (categories.data ?? []).filter((c) => c.kind === editing?.kind);

  const submit = () => {
    if (!editing) return;
    const accCur = (accounts.data ?? []).find((a) => a.id === editing.accountId)?.currency;
    const isForeign = editing.kind !== "transfer" && !!editing.origCurrency && editing.origCurrency !== accCur;
    const amount = isForeign ? editing.amountMinor : editing.origCurrency ? editing.origAmountMinor ?? editing.amountMinor : editing.amountMinor;
    void save.mutateAsync({
      ...editing,
      occurredAt: editing.occurredAt ?? nowLocalIso(),
      amountMinor: typeof amount === "number" ? amount : 0,
      origCurrency: isForeign ? editing.origCurrency : null,
      origAmountMinor: isForeign ? editing.origAmountMinor : null,
    });
  };

  const kindLabel = (k: string) => t(`ui.txKind.${k}`);

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

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.transactions")}</h2>
        <Button onClick={() => setEditing({ kind: "expense", occurredAt: nowLocalIso(), comment: "" })}>+ {t("ui.add")}</Button>
      </div>

      <div className="flex flex-wrap gap-2">
        <input className="flex-1 rounded border border-border bg-background px-3 py-2 text-sm"
          placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />
        <select className="rounded border border-border bg-background px-2 py-2 text-sm" value={accountId} onChange={(e) => setAccountId(e.target.value)}>
          <option value="">{t("ui.allAccounts")}</option>
          {(accounts.data ?? []).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
        <select className="rounded border border-border bg-background px-2 py-2 text-sm" value={categoryId} onChange={(e) => setCategoryId(e.target.value)}>
          <option value="">{t("ui.allCategories")}</option>
          {(categories.data ?? []).map((c) => <option key={c.id} value={c.id}>{catName(c)}</option>)}
        </select>
      </div>

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
            <Button size="sm" variant="outline" onClick={() => setEditing(null)}>{t("ui.cancel")}</Button>
            {save.isError && <span className="text-sm text-destructive">{String(save.error)}</span>}
          </div>
        </div>
      )}

      <div className="space-y-1">
        {(tx.data ?? []).map((x, i, arr) => {
          const day = x.occurredAt.slice(0, 10);
          const prev = i > 0 ? arr[i - 1].occurredAt.slice(0, 10) : null;
          return (
          <div key={x.id}>
            {day !== prev && <div className="pt-3 pb-1 text-xs font-semibold text-muted-foreground">{day}</div>}
          <div className="flex items-center gap-3 rounded-xl border border-border bg-card px-3 py-2 text-sm">
            {x.kind !== "transfer" && <CategoryIcon icon={x.cIcon} color={x.cColor} size={26} />}
            <span className={x.kind === "transfer" ? "text-muted-foreground" : x.kind === "income" ? "text-income" : "text-expense"}>
              {x.kind === "income" ? "+" : x.kind === "expense" ? "−" : "⇄"}
            </span>
            <span className="font-medium">
              {formatMinor(x.amountMinor, x.aCurrency)}
              {x.kind === "transfer" && x.toAmountMinor ? ` → ${formatMinor(x.toAmountMinor, x.a2Currency)}` : ""}
            </span>
            {x.origCurrency && x.origAmountMinor && (
              <span className="text-xs text-muted-foreground">({formatMinor(x.origAmountMinor, x.origCurrency)})</span>
            )}
            <span className="text-muted-foreground">
              {x.kind === "transfer" ? `${x.aName} → ${x.a2Name}` : `${x.aName} · ${catName(categories.data?.find((c) => c.id === x.categoryId))}`}
            </span>
            {x.comment && <span className="truncate text-xs text-muted-foreground">{x.comment}</span>}
            {(x.attCount ?? 0) > 0 && <Thumb txId={x.id} />}
            <span className="ml-auto whitespace-nowrap text-xs text-muted-foreground">
              {x.occurredAt.slice(0, 10)} {x.uName ? `· ${x.uName}` : ""} {x.source === "bot" ? "· 🤖" : ""}
            </span>
            <div className="flex gap-1">
              <Button size="sm" variant="ghost" onClick={() => setEditing(x)}>{t("ui.edit")}</Button>
              <Button size="sm" variant="ghost" onClick={() => del.mutate(x.id)}>{t("ui.delete")}</Button>
            </div>
          </div>
          </div>
          );
        })}
        {tx.data?.length === 0 && <p className="text-sm text-muted-foreground">{t("ui.empty")}</p>}
      </div>
    </div>
  );
}
