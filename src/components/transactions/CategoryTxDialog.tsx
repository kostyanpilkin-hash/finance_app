import { useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ArrowLeftRight } from "lucide-react";
import { api } from "@/lib/api";
import { formatMinor } from "@/lib/money";
import { qk } from "@/lib/queryKeys";
import { useDisplayMoney } from "@/lib/useDisplayMoney";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { CategoryIcon } from "@/components/ui/category-icon";
import { TxForm } from "@/components/transactions/TxForm";
import { cn } from "@/lib/utils";
import type { TransactionRow } from "@/lib/types";

export interface CategoryPeek {
  /** Dialog title (the slice's display name). */
  title: string;
  /** Category ids behind the slice; "" means "no category". */
  categoryIds?: string[];
  /** Entry owners behind the slice (spender, else author); "" means "no author". */
  ownerIds?: string[];
  accountId?: string;
  /** Omitted = both expenses and income (a day bar). */
  kind?: "expense" | "income";
  /** Narrower range than the page period (a clicked day). */
  from?: string;
  to?: string;
  /** The chart's own figures, shown as-is so the header matches the slice exactly. */
  total?: { expense?: number; income?: number; currency: string };
}

const PAGE = 200;

/** Popup listing the transactions behind a chart element; a row opens it for editing. */
export function CategoryTxDialog({
  peek,
  from,
  to,
  onClose,
}: {
  peek: CategoryPeek | null;
  from: string;
  to: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [editing, setEditing] = useState<TransactionRow | null>(null);
  const close = () => {
    setEditing(null);
    onClose();
  };

  return (
    <Dialog open={!!peek} onClose={close} title={peek?.title ?? ""} closeLabel={t("ui.close")} size={editing ? "lg" : "md"}>
      {peek && editing ? (
        <div className="space-y-3">
          <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>
            <ArrowLeft className="h-4 w-4" /> {t("ui.back")}
          </Button>
          <TxForm key={editing.id} initial={editing} onDone={() => setEditing(null)} />
        </div>
      ) : (
        peek && <PeekList peek={peek} from={peek.from ?? from} to={peek.to ?? to} onOpen={setEditing} />
      )}
    </Dialog>
  );
}

function PeekList({
  peek,
  from,
  to,
  onOpen,
}: {
  peek: CategoryPeek;
  from: string;
  to: string;
  onOpen: (x: TransactionRow) => void;
}) {
  const { t } = useTranslation();
  const { displayCurrency, toDisplay } = useDisplayMoney();
  const tx = useInfiniteQuery({
    queryKey: qk.txList("peek", from, to, peek.kind, peek.categoryIds, peek.ownerIds, peek.accountId),
    queryFn: ({ pageParam }) =>
      api.transactionList<TransactionRow>({
        from,
        to,
        kind: peek.kind,
        categoryIds: peek.categoryIds,
        ownerIds: peek.ownerIds,
        accountId: peek.accountId,
        limit: PAGE,
        offset: pageParam,
      }),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => (last.length === PAGE ? pages.length * PAGE : undefined),
  });
  // Charts never count transfers, so the list matches them.
  const rows = (tx.data?.pages.flat() ?? []).filter((x) => x.kind !== "transfer");
  const catName = (x: TransactionRow) =>
    x.cKey ? t(`categories.${x.cKey}`, x.cName ?? "") : x.cName || t("ui.noCategory");
  const byOwner = !!peek.ownerIds?.length;

  return (
    <div className="space-y-2">
      {peek.total && (
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-xl bg-muted/50 px-3 py-2 text-sm">
          <span className="text-muted-foreground">{t("ui.total")}:</span>
          {!!peek.total.expense && (
            <span className="font-semibold text-expense">−{formatMinor(peek.total.expense, peek.total.currency)}</span>
          )}
          {!!peek.total.income && (
            <span className="font-semibold text-income">+{formatMinor(peek.total.income, peek.total.currency)}</span>
          )}
          {!tx.isLoading && (
            <span className="ml-auto text-xs text-muted-foreground">
              {t("ui.peekCount", { count: rows.length, more: tx.hasNextPage ? "+" : "" })}
            </span>
          )}
        </div>
      )}
      {tx.isLoading && (
        <div className="space-y-2">
          <Skeleton className="h-11" />
          <Skeleton className="h-11" />
          <Skeleton className="h-11" />
        </div>
      )}
      {!tx.isLoading && rows.length === 0 && (
        <p className="py-6 text-center text-sm text-muted-foreground">{t("ui.empty")}</p>
      )}
      <div className="divide-y divide-border/50">
        {rows.map((x) => {
          const cur = x.aCurrency ?? "";
          const approx = cur && cur !== displayCurrency ? toDisplay(x.amountMinor, cur) : null;
          const who = x.suName ?? x.uName;
          const sub = [x.aName, byOwner ? catName(x) : who].filter(Boolean).join(" · ");
          return (
            <button
              key={x.id}
              type="button"
              onClick={() => onOpen(x)}
              aria-label={t("ui.openTx")}
              className="flex w-full cursor-pointer items-center gap-3 rounded-lg px-1 py-2 text-left transition-colors hover:bg-muted/60"
            >
              {x.kind === "transfer" ? (
                <ArrowLeftRight className="h-6 w-6 shrink-0 text-muted-foreground" />
              ) : (
                <CategoryIcon icon={x.cIcon} color={x.cColor} size={28} className="shrink-0" />
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm">{x.comment || catName(x)}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {x.occurredAt.slice(0, 10)}
                  {sub && ` · ${sub}`}
                </div>
              </div>
              <div className="shrink-0 text-right">
                <div className={cn("text-sm font-medium", x.kind === "income" ? "text-income" : "text-expense")}>
                  {x.kind === "income" ? "+" : "−"}
                  {formatMinor(x.amountMinor, cur)}
                </div>
                {approx != null && (
                  <div className="text-xs text-muted-foreground">≈ {formatMinor(approx, displayCurrency)}</div>
                )}
              </div>
            </button>
          );
        })}
      </div>
      {tx.hasNextPage && (
        <Button
          size="sm"
          variant="outline"
          className="w-full"
          disabled={tx.isFetchingNextPage}
          onClick={() => tx.fetchNextPage()}
        >
          {t("ui.showMore")}
        </Button>
      )}
    </div>
  );
}
