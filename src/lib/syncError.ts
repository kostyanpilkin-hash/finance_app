import type { TFunction } from "i18next";

export interface SyncLastError {
  code: string;
  hostSchema?: number;
  clientSchema?: number;
  message?: string;
}

/**
 * Localized text for a sync failure. Accepts either the structured lastError
 * from sync_status or a raw error string that may embed the code (join_host
 * wraps it). Returns null for an empty/absent error.
 */
export function syncErrorText(t: TFunction, e: SyncLastError | string | null | undefined): string | null {
  if (!e) return null;
  const code = typeof e === "string" ? e : e.code;
  // Schema details are appended only when the structured lastError carries
  // both numbers — a raw error string (join_host) never does.
  const schemas =
    typeof e === "object" && typeof e.clientSchema === "number" && typeof e.hostSchema === "number"
      ? ` ${t("ui.syncSchemas", { client: e.clientSchema, host: e.hostSchema })}`
      : "";
  if (code.includes("client_outdated")) {
    return t("ui.syncClientOutdated") + schemas;
  }
  if (code.includes("host_outdated")) {
    return t("ui.syncHostOutdated") + schemas;
  }
  const message = typeof e === "string" ? e : e.message;
  return t("ui.syncError", { message: message ?? code });
}

/**
 * Localized text for a pairing error (pair_to_host / join_host). The backend
 * returns short codes like "bad_code:3" or "pairing_locked"; anything
 * unrecognised is shown raw.
 */
export function pairErrorText(t: TFunction, err: string): string {
  if (err.startsWith("bad_code:")) {
    const left = err.slice("bad_code:".length);
    return t("pairing.badCode", { left });
  }
  if (err.includes("client_outdated") || err.includes("host_outdated")) {
    return syncErrorText(t, err) ?? err;
  }
  const key = (
    {
      pairing_locked: "pairing.lockedClient",
      pair_denied: "pairing.denied",
      pair_expired: "pairing.expired",
      pair_timeout: "pairing.expired",
      not_lan: "pairing.notLan",
    } as Record<string, string>
  )[err];
  return key ? t(key) : err;
}
