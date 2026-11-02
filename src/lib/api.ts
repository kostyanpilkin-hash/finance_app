import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  apiVersion: number;
  platform: string;
}

export interface OnboardingStatus {
  done: boolean;
  users: number;
  dbExisted: boolean;
  mode: "host" | "client";
}

export type EntityName =
  | "users" | "accounts" | "categories" | "transactions" | "attachments"
  | "goals" | "reminders" | "reminder_events" | "exchange_rates" | "audit_log";

export interface TransactionFilter {
  from?: string;
  to?: string;
  accountId?: string;
  categoryId?: string;
  kind?: string;
  search?: string;
  limit?: number;
  offset?: number;
  includeDeleted?: boolean;
}

export interface JournalFilter {
  entity?: string;
  entityId?: string;
  userId?: string;
  source?: string;
  action?: string;
  from?: string;
  to?: string;
  search?: string;
  limit?: number;
  offset?: number;
}

export interface CompleteOnboardingInput {
  userNames: string[];
  currentUser: number;
  starterAccounts: string[];
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  setTrayLabels: (open: string, quit: string, tooltip: string) =>
    invoke<void>("set_tray_labels", { open, quit, tooltip }),
  onboardingStatus: () => invoke<OnboardingStatus>("onboarding_status"),
  recoveryKey: () => invoke<string | null>("recovery_key"),
  completeOnboarding: (input: CompleteOnboardingInput) => invoke<void>("complete_onboarding", { input }),
  setCurrentUser: (userId: string) => invoke<void>("set_current_user", { userId }),
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  entityList: <T = unknown>(entity: EntityName, includeDeleted = false) =>
    invoke<T[]>("entity_list", { entity, includeDeleted }),
  entityDelete: (entity: EntityName, id: string) => invoke<void>("entity_delete", { entity, id }),
  entityRestore: (entity: EntityName, id: string) => invoke<void>("entity_restore", { entity, id }),
  userUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("user_upsert", { input }),
  accountUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("account_upsert", { input }),
  categoryUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("category_upsert", { input }),
  categoryDelete: (id: string, moveTo?: string) => invoke<void>("category_delete", { id, moveTo }),
  transactionUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("transaction_upsert", { input }),
  transactionList: <T = unknown>(filter: TransactionFilter) => invoke<T[]>("transaction_list", { filter }),
  dashboard: <T = unknown>(from: string, to: string) => invoke<T>("dashboard", { from, to }),
  journal: <T = unknown>(filter: JournalFilter) => invoke<T[]>("journal", { filter }),
  goalUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("goal_upsert", { input }),
  attachmentAdd: <T = unknown>(transactionId: string, fileName: string, mime: string, bytesBase64: string) =>
    invoke<T>("attachment_add", { transactionId, fileName, mime, bytesBase64 }),
  attachmentPath: (sha256: string) => invoke<string>("attachment_path", { sha256 }),
  ratesLatest: <T = unknown>() => invoke<T[]>("rates_latest"),
  ratesRefresh: () => invoke<{ updated: string[]; skipped: string[] }>("rates_refresh"),
  exportXlsx: (from: string, to: string) => invoke<string>("export_xlsx", { from, to }),
};
