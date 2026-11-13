import { invoke } from "@tauri-apps/api/core";
import type { AnalyticsSummary, Bucket } from "./types";

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
  /** Multi-category filter; an empty string member matches entries without a category. */
  categoryIds?: string[];
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
  transactionDays: (from: string, to: string) => invoke<string[]>("transaction_days", { from, to }),
  dashboard: <T = unknown>(from: string, to: string) => invoke<T>("dashboard", { from, to }),
  analyticsSummary: (from: string, to: string, currency?: string) =>
    invoke<AnalyticsSummary>("analytics_summary", { from, to, currency }),
  analyticsMonthly: (from: string, to: string, currency?: string) =>
    invoke<Bucket[]>("analytics_monthly", { from, to, currency }),
  journal: <T = unknown>(filter: JournalFilter) => invoke<T[]>("journal", { filter }),
  goalUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("goal_upsert", { input }),
  reminderUpsert: <T = unknown>(input: Record<string, unknown>) => invoke<T>("reminder_upsert", { input }),
  reminderResolve: (id: string, status: string, createTx?: Record<string, unknown>) =>
    invoke<void>("reminder_resolve", { id, status, createTx }),
  remindersDue: <T = unknown>() => invoke<T[]>("reminders_due"),
  pinSet: (pin: string) => invoke<void>("pin_set", { pin }),
  pinVerify: (pin: string) => invoke<boolean>("pin_verify", { pin }),
  pinClear: () => invoke<void>("pin_clear"),
  pinEnabled: () => invoke<boolean>("pin_enabled"),
  backupCreate: () => invoke<string>("backup_create"),
  backupList: () => invoke<string[]>("backup_list"),
  backupRestore: (name: string) => invoke<void>("backup_restore", { name }),
  /** Sends photo bytes as a raw IPC payload (no base64); metadata rides in headers. */
  attachmentAdd: <T = unknown>(transactionId: string, fileName: string, mime: string, bytes: Uint8Array) =>
    invoke<T>("attachment_add", bytes, {
      headers: {
        "x-transaction-id": transactionId,
        "x-file-name": encodeURIComponent(fileName),
        "x-mime": mime,
      },
    }),
  attachmentPath: (sha256: string) => invoke<string>("attachment_path", { sha256 }),
  ratesLatest: <T = unknown>() => invoke<T[]>("rates_latest"),
  ratesRefresh: () => invoke<{ updated: string[]; skipped: string[] }>("rates_refresh"),
  convertAt: (amountMinor: number, from: string, to: string, date: string) =>
    invoke<number | null>("convert_at", { amountMinor, from, to, date }),
  exportXlsx: (from: string, to: string) => invoke<string>("export_xlsx", { from, to }),
  tgConnect: (token: string) => invoke<{ username: string; botId: string }>("tg_connect", { token }),
  tgDisconnect: () => invoke<void>("tg_disconnect"),
  tgStatus: () => invoke<{
    enabled: boolean; username?: string; linked: [string, number][];
    health: "ok" | "offline" | "stopped"; logPath?: string | null;
    status?: { running: boolean; lastPoll?: string | null; lastMessage?: string | null; lastError?: string | null; lastErrorAt?: string | null };
  }>("tg_status"),
  tgPairCode: (userId: string) => invoke<string>("tg_pair_code", { userId }),
  devicePairCode: (userId: string) => invoke<string>("device_pair_code", { userId }),
  pairToHost: (hostUrl: string, code: string, name: string) =>
    invoke<void>("pair_to_host", { hostUrl, code, name }),
  pairingRequests: () =>
    invoke<{
      requests: { id: string; userId: string; name: string; platform: string; ip: string; createdAt: string }[];
      locked: boolean;
      failures: number;
    }>("pairing_requests"),
  pairingDecide: (requestId: string, allow: boolean) =>
    invoke<void>("pairing_decide", { requestId, allow }),
  discoverHosts: () => invoke<{ name: string; host: string; url: string }[]>("discover_hosts"),
  hostAddresses: () => invoke<string[]>("host_addresses"),
  conflictList: () => invoke<{ id: string; entity: string; entityId: string; local: string; server: string }[]>("conflict_list"),
  conflictResolve: (conflictId: string, winner: "local" | "server") =>
    invoke<void>("conflict_resolve", { conflictId, winner }),
  syncStatus: () =>
    invoke<{
      mode: string; hostUrl?: string; lastSeq?: string; pendingOps: number; pairedDevices: number;
      lastError?: { code: string; hostSchema?: number; clientSchema?: number; message?: string } | null;
    }>("sync_status"),
  joinHost: (hostUrl: string, code: string, name: string) =>
    invoke<void>("join_host", { hostUrl, code, name }),
  dbStatus: () => invoke<"ready" | "locked" | "error">("db_status"),
  startupError: () => invoke<{ message: string; dataDir: string } | null>("startup_error"),
  unlockDb: (recovery: string) => invoke<void>("unlock_db", { recovery }),
};
