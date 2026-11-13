import type { TransactionRow } from "./types";

/** i18n key for the owner label by entry kind: spent / received / whose. */
export function ownerLabelKey(kind: string): string {
  return kind === "income" ? "ui.receivedBy" : kind === "transfer" ? "ui.whoseOp" : "ui.spentBy";
}

/** Entry owner: the explicit owner when set, else the author. */
export function ownerOf(x: TransactionRow, nameOf?: (id: string) => string | undefined) {
  const id = x.spentByUserId ?? x.createdByUserId ?? "";
  const name = x.suName ?? x.uName ?? nameOf?.(id) ?? "";
  return { id, name };
}
