export interface UserRow {
  id: string;
  name: string;
  color: string;
}

export type PaymentSystem = "visa" | "mastercard" | "belkart" | "mir" | "unionpay";

export interface AccountRow {
  id: string;
  name: string;
  kind: "card" | "cash" | "savings" | "other";
  last4?: string | null;
  bank?: string | null;
  paymentSystem?: PaymentSystem | null;
  currency: string;
  color: string;
  ownerUserId?: string | null;
  includeInTotal: number;
  archived: number;
  sort: number;
  balance?: number;
  ownerName?: string;
}

export interface CategoryRow {
  id: string;
  name: string;
  key?: string | null;
  kind: "expense" | "income";
  icon: string;
  color: string;
  archived: number;
  isSystem: number;
}

export interface TransactionRow {
  id: string;
  kind: "expense" | "income" | "transfer";
  occurredAt: string;
  accountId: string;
  amountMinor: number;
  categoryId?: string | null;
  toAccountId?: string | null;
  toAmountMinor?: number | null;
  comment: string;
  /** Amount as entered when its currency differs from the account currency. */
  origAmountMinor?: number | null;
  origCurrency?: string | null;
  createdByUserId?: string | null;
  source: string;
  aName?: string;
  aCurrency?: string;
  aColor?: string;
  a2Name?: string;
  a2Currency?: string;
  cName?: string;
  cIcon?: string;
  cColor?: string;
  cKey?: string;
  uName?: string;
  attCount?: number;
}

export interface AuditRow {
  id: string;
  ts: string;
  userId?: string | null;
  source: string;
  entity: string;
  entityId: string;
  action: string;
  beforeJson?: string | null;
  afterJson?: string | null;
  changesJson?: string | null;
  uName?: string;
}

export interface DashboardData {
  accounts: AccountRow[];
  totals: { kind: string; currency: string; amount: number }[];
}
