import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { formatMinor } from "@/lib/money";
import { Dialog } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { TransactionRow } from "@/lib/types";

export interface CategoryPeek {
  /** Dialog title (the slice's display name). */
  title: string;
  /** Category ids behind the slice; "" means "no category". */
  categoryIds: string[];
  kind: "expense" | "income";
}

/** Read-only popup listing the transactions behind a category chart slice. */
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
  const tx = useQuery({
    queryKey: ["transactions", "cat-peek", from, to, peek?.kind, peek?.categoryIds],
    queryFn: () =>
      api.transactionList<TransactionRow>({
        from,
        to,
        kind: peek?.kind,
        categoryIds: peek?.categoryIds,
        limit: 500,
      }),
    enabled: !!peek,
  });

  return (
    <Dialog open={!!peek} onClose={onClose} title={peek?.title ?? ""} closeLabel={t("ui.close")}>
      {tx.isLoading && (
        <div className="space-y-2">
          <Skeleton className="h-9" />
          <Skeleton className="h-9" />
          <Skeleton className="h-9" />
        </div>
      )}
      {!tx.isLoading && (tx.data?.length ?? 0) === 0 && (
        <p className="py-6 text-center text-sm text-muted-foreground">{t("ui.empty")}</p>
      )}
      <div className="divide-y divide-border/50">
        {(tx.data ?? []).map((x) => (
          <div key={x.id} className="flex items-center gap-3 py-2">
            <div className="w-20 shrink-0 text-xs text-muted-foreground">
              {x.occurredAt.slice(0, 10)}
            </div>
            <div className="min-w-0 flex-1 truncate text-sm">{x.comment || x.aName || "—"}</div>
            <div
              className={cn(
                "shrink-0 text-sm font-medium",
                x.kind === "income" ? "text-income" : "text-expense",
              )}
            >
              {x.kind === "income" ? "+" : "−"}
              {formatMinor(x.amountMinor, x.aCurrency)}
            </div>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
