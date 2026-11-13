import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRightLeft, CalendarDays, ChevronLeft, ChevronRight, Pencil, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { CategoryIcon } from "@/components/ui/category-icon";
import { TxForm } from "@/components/transactions/TxForm";
import { DayCalendar } from "@/components/transactions/DayCalendar";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/stores/toast";
import { useDebounced } from "@/lib/useDebounced";
import { cn } from "@/lib/utils";
import { formatMinor, nowLocalIso, todayIso } from "@/lib/money";
import { ownerLabelKey } from "@/lib/owners";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AccountRow, CategoryRow, TransactionRow } from "@/lib/types";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";

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
  return <img src={convertFileSrc(path.data)} alt="" className="h-7 w-7 rounded-md object-cover ring-1 ring-border" />;
}

function isoOf(d: Date) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function TransactionsPage() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [accountId, setAccountId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [day, setDay] = useState(todayIso());
  const [calOpen, setCalOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const accounts = useQuery({ queryKey: ["accounts"], queryFn: () => api.entityList<AccountRow>("accounts") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api.entityList<CategoryRow>("categories") });

  // Debounced so a keystroke doesn't hit the whole history every time.
  const q = useDebounced(search.trim());
  const searching = q.length > 0;
  const tx = useQuery({
    queryKey: ["transactions", "page", searching ? `search:${q}` : `day:${day}`, accountId, categoryId],
    queryFn: () =>
      api.transactionList<TransactionRow>(
        searching
          ? { search: q, accountId: accountId || undefined, categoryId: categoryId || undefined, limit: 500 }
          : { from: day, to: day, accountId: accountId || undefined, categoryId: categoryId || undefined, limit: 500 },
      ),
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["transactions"] });
    qc.invalidateQueries({ queryKey: ["tx-days"] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const del = useMutation({
    mutationFn: (id: string) => api.entityDelete("transactions", id),
    onSuccess: invalidate,
  });
  const restore = useMutation({
    mutationFn: (id: string) => api.entityRestore("transactions", id),
    onSuccess: invalidate,
  });
  const remove = (id: string) => {
    del.mutate(id);
    toast(t("ui.txDeleted"), { actionLabel: t("ui.undo"), onAction: () => restore.mutate(id) });
  };

  const catName = (c?: CategoryRow) => (c ? (c.key ? t(`categories.${c.key}`, c.name) : c.name) : "—");

  const dayGroups = useMemo(() => {
    const groups: { day: string; items: TransactionRow[]; totals: Record<string, number> }[] = [];
    for (const x of tx.data ?? []) {
      const d = x.occurredAt.slice(0, 10);
      let g = groups[groups.length - 1];
      if (!g || g.day !== d) {
        g = { day: d, items: [], totals: {} };
        groups.push(g);
      }
      g.items.push(x);
      const cur = x.aCurrency ?? "";
      if (x.kind === "expense") g.totals[cur] = (g.totals[cur] ?? 0) - x.amountMinor;
      else if (x.kind === "income") g.totals[cur] = (g.totals[cur] ?? 0) + x.amountMinor;
    }
    return groups;
  }, [tx.data]);

  const dayLabel = (d: string) => {
    const dm = new Date(`${d}T00:00:00`).toLocaleDateString(i18n.language, { day: "numeric", month: "long" });
    const y = new Date();
    y.setDate(y.getDate() - 1);
    if (d === todayIso()) return `${t("ui.today")}, ${dm}`;
    if (d === isoOf(y)) return `${t("ui.yesterday")}, ${dm}`;
    return dm;
  };

  const shiftDay = (n: number) => {
    const d = new Date(`${day}T00:00:00`);
    d.setDate(d.getDate() + n);
    setDay(isoOf(d));
  };

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.transactions")}</h2>
        <Button onClick={() => { setAddOpen(true); setEditingId(null); }}>+ {t("ui.add")}</Button>
      </div>

      <div className="flex flex-wrap gap-2">
        <Input className="flex-1"
          placeholder={t("ui.searchComment")} value={search} onChange={(e) => setSearch(e.target.value)} />
        <Select value={accountId} onChange={(e) => setAccountId(e.target.value)}>
          <option value="">{t("ui.allAccounts")}</option>
          {(accounts.data ?? []).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </Select>
        <Select value={categoryId} onChange={(e) => setCategoryId(e.target.value)}>
          <option value="">{t("ui.allCategories")}</option>
          {(categories.data ?? []).map((c) => <option key={c.id} value={c.id}>{catName(c)}</option>)}
        </Select>
      </div>

      {searching ? (
        <div className="px-1 text-sm text-muted-foreground">
          {t("ui.searchResults")}: {tx.data?.length ?? "…"}
        </div>
      ) : (
        <div className="relative flex items-center justify-center gap-1">
          <button type="button" className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg transition-colors hover:bg-muted" onClick={() => shiftDay(-1)}>
            <ChevronLeft className="h-4 w-4" />
          </button>
          <button
            type="button"
            className={cn(
              "flex h-9 cursor-pointer items-center gap-2 rounded-xl border px-3 py-1.5 text-sm font-medium transition-colors",
              calOpen ? "border-primary bg-primary/10" : "border-border bg-card hover:bg-muted/60",
            )}
            onClick={() => setCalOpen((v) => !v)}
          >
            <CalendarDays className="h-4 w-4 text-muted-foreground" />
            {dayLabel(day)}
          </button>
          <button type="button" className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg transition-colors hover:bg-muted" onClick={() => shiftDay(1)}>
            <ChevronRight className="h-4 w-4" />
          </button>
          {calOpen && (
            <>
              <div className="fixed inset-0 z-10" onClick={() => setCalOpen(false)} />
              <DayCalendar
                selected={day}
                onPick={(d) => {
                  setDay(d);
                  setCalOpen(false);
                }}
              />
            </>
          )}
        </div>
      )}

      {addOpen && (
        <TxForm
          initial={{ kind: "expense", occurredAt: `${day}T${nowLocalIso().slice(11)}`, comment: "" }}
          onDone={() => setAddOpen(false)}
        />
      )}

      <div className="space-y-5">
        {tx.isLoading && (
          <div className="space-y-3">
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-36 w-full rounded-2xl" />
          </div>
        )}
        {dayGroups.map((g) => (
          <section key={g.day}>
            <div className="mb-1.5 flex items-baseline justify-between gap-3 px-1">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{dayLabel(g.day)}</h3>
              <div className="text-xs font-semibold">
                {Object.entries(g.totals).map(([cur, v], i) => (
                  <span key={cur} className={cn(v < 0 ? "text-expense" : v > 0 ? "text-income" : "text-muted-foreground")}>
                    {i > 0 && "  ·  "}
                    {v > 0 ? "+" : ""}{formatMinor(v, cur)}
                  </span>
                ))}
              </div>
            </div>
            <div className="divide-y divide-border/50 overflow-hidden rounded-2xl border border-border/80 bg-card/90 shadow-[var(--card-shadow)]">
              {g.items.map((x) => {
                const cat = categories.data?.find((c) => c.id === x.categoryId);
                return (
                  <div key={x.id}>
                    <div
                      className={cn(
                        "group flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors hover:bg-muted/50",
                        editingId === x.id && "bg-muted/50",
                      )}
                      onClick={() => {
                        setAddOpen(false);
                        setEditingId(editingId === x.id ? null : x.id);
                      }}
                    >
                      {x.kind === "transfer" ? (
                        <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
                          <ArrowRightLeft className="h-4 w-4" />
                        </span>
                      ) : (
                        <CategoryIcon icon={x.cIcon} color={x.cColor} size={32} />
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium">
                          {x.kind === "transfer" ? `${x.aName} → ${x.a2Name}` : catName(cat)}
                        </div>
                        <div className="truncate text-xs text-muted-foreground">
                          {[x.kind !== "transfer" ? x.aName : null, x.comment].filter(Boolean).join(" · ") || " "}
                        </div>
                      </div>
                      {(x.attCount ?? 0) > 0 && <Thumb txId={x.id} />}
                      <div className="shrink-0 text-right">
                        <div className={cn(
                          "text-sm font-semibold",
                          x.kind === "income" ? "text-income" : x.kind === "expense" ? "text-expense" : "text-muted-foreground",
                        )}>
                          {x.kind === "income" ? "+" : x.kind === "expense" ? "−" : ""}
                          {formatMinor(x.amountMinor, x.aCurrency)}
                          {x.kind === "transfer" && x.toAmountMinor ? ` → ${formatMinor(x.toAmountMinor, x.a2Currency)}` : ""}
                        </div>
                        {x.origCurrency && x.origAmountMinor ? (
                          <div className="text-xs text-muted-foreground">{formatMinor(x.origAmountMinor, x.origCurrency)}</div>
                        ) : null}
                        <div className="text-xs text-muted-foreground">
                          {t("ui.time")}: {x.occurredAt.slice(11, 16)}{x.source === "bot" ? " 🤖" : ""}
                        </div>
                        <div className="text-xs text-muted-foreground">{t("ui.entered")}: {x.uName || "—"}</div>
                        <div className="text-xs text-muted-foreground">
                          {t(ownerLabelKey(x.kind))}: {x.suName || x.uName || "—"}
                        </div>
                      </div>
                      <div className="flex shrink-0 gap-0.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                        <Button size="icon-sm" variant="ghost" title={t("ui.edit")}
                          onClick={(e) => {
                            e.stopPropagation();
                            setAddOpen(false);
                            setEditingId(editingId === x.id ? null : x.id);
                          }}>
                          <Pencil className="h-3.5 w-3.5" />
                        </Button>
                        <Button size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" title={t("ui.delete")}
                          onClick={(e) => {
                            e.stopPropagation();
                            remove(x.id);
                          }}>
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>
                    {editingId === x.id && (
                      <div className="bg-muted/30 px-4 py-3">
                        <TxForm initial={x} onDone={() => setEditingId(null)} />
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        ))}
        {tx.data?.length === 0 && (
          <p className="text-sm text-muted-foreground">
            {searching ? t("ui.empty") : t("ui.noOpsThisDay")}
          </p>
        )}
      </div>
    </div>
  );
}
