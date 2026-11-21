export interface UserRow {
  id: string;
  name: string;
  color: string;
}

/** Rate kinds a conversion can use (`finance_core::fx::RateKind`). */
export type RateKind = "bsb_app" | "bsb_card" | "nbrb";
/** What filled a converted amount; `manual` = typed by the user. */
export type RateSource = RateKind | "manual";
/** Account conversion bank; null = by account kind. */
export type AccountRateSource = "bsb" | "nbrb";

export type PaymentSystem = "visa" | "mastercard" | "belkart" | "mir" | "unionpay";

export interface AccountRow {
  id: string;
  name: string;
  kind: "card" | "cash" | "savings" | "other";
  last4?: string | null;
  bank?: string | null;
  paymentSystem?: PaymentSystem | null;
  rateSource?: AccountRateSource | null;
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
  rateSource?: RateSource | null;
  createdByUserId?: string | null;
  /** User the entry belongs to (every kind; falls back to the author). */
  spentByUserId?: string | null;
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
  /** Entry-owner name (from the spent_by_user_id join). */
  suName?: string;
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

export interface Bucket {
  key: string | null;
  income: number;
  expense: number;
}

export interface AnalyticsSummary {
  currency: string;
  income: number;
  expense: number;
  missing: string[];
  byDay: Bucket[];
  byCategory: Bucket[];
  byOwner: Bucket[];
  byAccount: Bucket[];
}

export interface FxSuggestReq {
  kind: TransactionRow["kind"];
  accountId: string;
  toAccountId?: string | null;
  enteredCurrency?: string | null;
  amountMinor: number;
  occurredAt: string;
  rateKind?: RateKind | null;
}

export interface FxSuggestion {
  amountMinor: number;
  currency: string;
  rateKind: RateKind;
  defaultKind: RateKind;
  mode: "fill" | "reference";
  fallback: boolean;
  /** BYN per 1 unit of the foreign currency ×1e6. */
  rateScaled: number | null;
}

export interface RateOverview {
  currency: string;
  bsbApp: { buy: number; sell: number } | null;
  bsbCard: { buy: number; sell: number } | null;
  /** NBRB per 1 unit ×1e6. */
  nbrb: number | null;
}
