import type { DonutItem } from "@/components/ui/donut-chart";
import type { CategoryPeek } from "@/components/transactions/CategoryTxDialog";

/** Turns a clicked donut slice into the transaction-list filter behind it. */
export function slicePeek(
  kind: "expense" | "income",
  by: "category" | "owner",
  members: DonutItem[],
  label: string,
  currency: string,
): CategoryPeek {
  const ids = members.map((m) => m.id ?? "");
  const sum = members.reduce((s, m) => s + m.value, 0);
  return {
    title: label,
    kind,
    ...(by === "category" ? { categoryIds: ids } : { ownerIds: ids }),
    total: { [kind]: sum, currency },
  };
}
