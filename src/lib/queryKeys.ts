import type { QueryClient } from "@tanstack/react-query";
import type { EntityName } from "./api";

export const qk = {
  entity: (e: EntityName, includeDeleted = false) => ["entity", e, includeDeleted] as const,
  entityAll: (e: EntityName) => ["entity", e] as const,
  transactions: ["transactions"] as const,
  txList: (scope: string, ...parts: unknown[]) => ["transactions", scope, ...parts] as const,
  txDays: (month: string) => ["transactions", "days", month] as const,
  dashboard: (...parts: unknown[]) => ["dashboard", ...parts] as const,
  analytics: (...parts: unknown[]) => ["analytics", ...parts] as const,
  journal: (...parts: unknown[]) => ["journal", ...parts] as const,
  settings: ["settings"] as const,
  rates: ["rates"] as const,
  remindersDue: ["reminders-due"] as const,
  onboarding: ["onboarding"] as const,
  pinEnabled: ["pin-enabled"] as const,
  appInfo: ["app-info"] as const,
  backups: ["backups"] as const,
  syncStatus: ["sync-status"] as const,
  pairingRequests: ["pairing-requests"] as const,
  conflicts: ["conflicts"] as const,
  devices: ["devices"] as const,
  tgStatus: ["tg-status"] as const,
  attachmentPath: (sha: string | undefined) => ["attachment-path", sha] as const,
};

/** Keys that must NOT refetch on "data-changed" (slow or unrelated to the db). */
export const NON_DATA_ROOTS = new Set([
  "db-status", "app-info", "autostart", "discover-hosts", "recovery-key", "host-addresses", "startup-error", "attachment-path",
]);

/** Everything a money movement can affect. */
export function invalidateMoney(qc: QueryClient) {
  for (const key of [
    qk.transactions, qk.dashboard(), qk.analytics(), qk.journal(),
    qk.entityAll("accounts"), qk.entityAll("attachments"), qk.entityAll("goals"),
  ]) {
    void qc.invalidateQueries({ queryKey: key });
  }
}
