import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Check } from "lucide-react";
import { LANGUAGES } from "@/i18n";
import { api } from "@/lib/api";
import { THEMES, type ThemeKind } from "@/lib/themes";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-2xl border border-border/80 bg-card/90 p-5 shadow-[var(--card-shadow)]">
      <h3 className="font-display mb-4 text-lg font-semibold">{title}</h3>
      {children}
    </section>
  );
}

interface RateRow { currency: string; rateBynScaled: number; scale: number; date: string }

function RatesSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const rates = useQuery({ queryKey: ["rates"], queryFn: () => api.ratesLatest<RateRow>() });
  const refresh = useMutation({
    mutationFn: api.ratesRefresh,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["rates"] }),
  });
  return (
    <div className="space-y-2 text-sm">
      <Button size="sm" onClick={() => refresh.mutate()} disabled={refresh.isPending}>
        {refresh.isPending ? "…" : t("ui.refreshRates")}
      </Button>
      {(rates.data ?? []).map((r) => (
        <div key={r.currency} className="flex justify-between border-b border-border/50 pb-1">
          <span>{r.currency}</span>
          <span className="text-muted-foreground">
            {(r.rateBynScaled / 1e6).toFixed(4)} BYN · {r.date}
          </span>
        </div>
      ))}
      {refresh.data && (
        <p className="text-xs text-muted-foreground">
          {t("ui.refreshed")}: {refresh.data.updated.join(", ")}
          {refresh.data.skipped.length > 0 && ` · ${t("ui.skipped")}: ${refresh.data.skipped.join(", ")}`}
        </p>
      )}
    </div>
  );
}

function TgSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [token, setToken] = useState("");
  const status = useQuery({ queryKey: ["tg-status"], queryFn: api.tgStatus });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<{ id: string; name: string }>("users") });
  const [code, setCode] = useState<Record<string, string>>({});
  const connect = useMutation({
    mutationFn: () => api.tgConnect(token),
    onSuccess: () => { setToken(""); queryClient.invalidateQueries({ queryKey: ["tg-status"] }); },
  });
  const disconnect = useMutation({
    mutationFn: api.tgDisconnect,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["tg-status"] }),
  });
  const pair = useMutation({
    mutationFn: (userId: string) => api.tgPairCode(userId),
    onSuccess: (c, userId) => setCode((s) => ({ ...s, [userId]: c })),
  });
  const s = status.data;
  return (
    <div className="space-y-3 text-sm">
      {!s?.enabled ? (
        <div className="space-y-2">
          <input className="w-full rounded border border-border bg-background px-3 py-2" type="password"
            placeholder="Bot token (BotFather)" value={token} onChange={(e) => setToken(e.target.value)} />
          <Button size="sm" onClick={() => connect.mutate()} disabled={!token.trim() || connect.isPending}>
            {t("ui.connect")}
          </Button>
          {connect.isError && <p className="text-destructive">{String(connect.error)}</p>}
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3">
            <span>@{s.username} — {t("ui.connected")}</span>
            <Button size="sm" variant="outline" onClick={() => disconnect.mutate()}>{t("ui.disconnect")}</Button>
          </div>
          <div className="space-y-2">
            {(users.data ?? []).map((u) => {
              const linked = s.linked?.some(([uid]) => uid === u.id);
              return (
                <div key={u.id} className="flex items-center gap-3">
                  <span className="w-32">{u.name}</span>
                  {linked ? (
                    <span className="text-income">{t("ui.tgLinked")}</span>
                  ) : code[u.id] ? (
                    <code className="rounded bg-muted px-2 py-1 text-lg font-semibold tracking-widest">{code[u.id]}</code>
                  ) : (
                    <Button size="sm" variant="outline" onClick={() => pair.mutate(u.id)}>{t("ui.tgPair")}</Button>
                  )}
                </div>
              );
            })}
            {pair.isPending === false && Object.keys(code).length > 0 && (
              <p className="text-xs text-muted-foreground">{t("ui.tgPairHint")}</p>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function SyncSection() {
  const { t } = useTranslation();
  const status = useQuery({ queryKey: ["sync-status"], queryFn: api.syncStatus, refetchInterval: 10000 });
  const users = useQuery({ queryKey: ["users"], queryFn: () => api.entityList<{ id: string; name: string }>("users") });
  const [hostUrl, setHostUrl] = useState("http://192.168.");
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [pairCode, setPairCode] = useState<Record<string, string>>({});
  const queryClient = useQueryClient();
  const pair = useMutation({
    mutationFn: () => api.pairToHost(hostUrl, code, name),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["sync-status"] }),
  });
  const makeCode = useMutation({
    mutationFn: (uid: string) => api.devicePairCode(uid),
    onSuccess: (c, uid) => setPairCode((s) => ({ ...s, [uid]: c })),
  });
  const s = status.data;
  return (
    <div className="space-y-4 text-sm">
      <div>
        <div className="mb-1 font-medium">{t("ui.hostMode")}</div>
        {(users.data ?? []).map((u) => (
          <div key={u.id} className="flex items-center gap-3">
            <span className="w-32">{u.name}</span>
            {pairCode[u.id] ? (
              <code className="rounded bg-muted px-2 py-1 text-lg font-semibold tracking-widest">{pairCode[u.id]}</code>
            ) : (
              <Button size="sm" variant="outline" onClick={() => makeCode.mutate(u.id)}>{t("ui.pairDevice")}</Button>
            )}
          </div>
        ))}
        <p className="mt-1 text-xs text-muted-foreground">
          {t("ui.syncStatus")}: {s?.pairedDevices ?? 0} · {s?.pendingOps ?? 0} {t("ui.pendingOps")}
        </p>
      </div>
      <div>
        <div className="mb-1 font-medium">{t("ui.joinHost")}</div>
        <div className="flex flex-wrap gap-2">
          <input className="flex-1 rounded border border-border bg-background px-3 py-2" placeholder="http://192.168.1.10:45322"
            value={hostUrl} onChange={(e) => setHostUrl(e.target.value)} />
          <input className="w-24 rounded border border-border bg-background px-3 py-2" placeholder={t("ui.code")}
            value={code} onChange={(e) => setCode(e.target.value)} />
          <input className="w-32 rounded border border-border bg-background px-3 py-2" placeholder={t("ui.deviceName")}
            value={name} onChange={(e) => setName(e.target.value)} />
          <Button size="sm" variant="outline" onClick={async () => {
            const hosts = await api.discoverHosts();
            if (hosts[0]?.url) setHostUrl(hosts[0].url);
          }}>{t("ui.findHost")}</Button>
          <Button size="sm" onClick={() => pair.mutate()} disabled={pair.isPending}>{t("ui.connect")}</Button>
        </div>
        {pair.isError && <p className="mt-1 text-destructive">{String(pair.error)}</p>}
        {s?.hostUrl && <p className="mt-1 text-xs text-muted-foreground">{s.hostUrl} · seq {s.lastSeq}</p>}
      </div>
      <ConflictsBlock />
    </div>
  );
}

function ConflictsBlock() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: ["conflicts"], queryFn: api.conflictList, refetchInterval: 15000 });
  const resolve = useMutation({
    mutationFn: ({ id, winner }: { id: string; winner: "local" | "server" }) => api.conflictResolve(id, winner),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["conflicts"] });
      queryClient.invalidateQueries({ queryKey: ["sync-status"] });
    },
  });
  if (!list.data?.length) return null;
  const short = (json: string) => {
    try {
      const v = JSON.parse(json);
      const name = v.name ?? v.amountMinor ?? v.payload?.name ?? v.payload?.amountMinor ?? "";
      return name !== "" ? String(name) : JSON.stringify(v).slice(0, 60);
    } catch {
      return json.slice(0, 60);
    }
  };
  return (
    <div>
      <div className="mb-1 font-medium text-destructive">{t("ui.conflicts")}: {list.data.length}</div>
      {list.data.map((c) => (
        <div key={c.id} className="mb-2 space-y-1 rounded-xl border border-destructive/40 bg-card p-3">
          <div className="text-xs text-muted-foreground">{c.entity} · {t("ui.conflictHint")}</div>
          <div className="flex items-center gap-2">
            <span className="flex-1 truncate">{t("ui.variantA")}: {short(c.local)}</span>
            <Button size="sm" onClick={() => resolve.mutate({ id: c.id, winner: "local" })}>{t("ui.keepA")}</Button>
            <span className="flex-1 truncate">{t("ui.variantB")}: {short(c.server)}</span>
            <Button size="sm" variant="outline" onClick={() => resolve.mutate({ id: c.id, winner: "server" })}>{t("ui.keepB")}</Button>
          </div>
        </div>
      ))}
    </div>
  );
}

function SecuritySection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const enabled = useQuery({ queryKey: ["pin-enabled"], queryFn: api.pinEnabled });
  const [pin, setPin] = useState("");
  const set_ = useMutation({
    mutationFn: () => api.pinSet(pin),
    onSuccess: () => { setPin(""); qc.invalidateQueries({ queryKey: ["pin-enabled"] }); },
  });
  const clear = useMutation({
    mutationFn: api.pinClear,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["pin-enabled"] }),
  });
  return (
    <div className="space-y-3 text-sm">
      {enabled.data ? (
        <div className="flex items-center gap-3">
          <span>{t("ui.pinOn")}</span>
          <Button size="sm" variant="outline" onClick={() => clear.mutate()}>{t("ui.pinOff")}</Button>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <input className="w-36 rounded border border-border bg-background px-3 py-2" type="password" inputMode="numeric"
            maxLength={8} placeholder="PIN (4–8)" value={pin}
            onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))} />
          <Button size="sm" onClick={() => set_.mutate()} disabled={pin.length < 4 || set_.isPending}>{t("ui.setPin")}</Button>
        </div>
      )}
      <p className="text-xs text-muted-foreground">{t("ui.pinHint")}</p>
    </div>
  );
}

function BackupSection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["backups"], queryFn: api.backupList });
  const create = useMutation({
    mutationFn: api.backupCreate,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["backups"] }),
  });
  const restore = useMutation({ mutationFn: api.backupRestore });
  return (
    <div className="space-y-3 text-sm">
      <div className="flex items-center gap-3">
        <Button size="sm" onClick={() => create.mutate()} disabled={create.isPending}>{t("ui.backupNow")}</Button>
        {create.data && <span className="text-xs text-muted-foreground">{create.data}</span>}
      </div>
      <p className="text-xs text-muted-foreground">{t("ui.backupHint")}</p>
      <div className="space-y-1">
        {(list.data ?? []).slice(0, 12).map((b) => (
          <div key={b} className="flex items-center justify-between rounded-lg bg-muted px-3 py-1.5 font-mono text-xs">
            <span>{b}</span>
            <button className="text-primary hover:underline" onClick={() => {
              if (window.confirm(t("ui.restoreConfirm"))) restore.mutate(b);
            }}>{t("ui.restore")}</button>
          </div>
        ))}
        {restore.isSuccess && <p className="text-xs text-primary">{t("ui.restoreRestart")}</p>}
        {restore.isError && <p className="text-xs text-destructive">{String(restore.error)}</p>}
        {list.data?.length === 0 && <p className="text-xs text-muted-foreground">{t("ui.noBackups")}</p>}
      </div>
    </div>
  );
}

export function SettingsPage() {
  const { t } = useTranslation();
  const { theme, language, setTheme, setLanguage } = useSettingsStore();
  const queryClient = useQueryClient();

  const info = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const autostart = useQuery({ queryKey: ["autostart"], queryFn: isEnabled });
  const toggleAutostart = useMutation({
    mutationFn: (on: boolean) => (on ? enable() : disable()),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["autostart"] }),
  });

  const kinds: ThemeKind[] = ["light", "dark"];

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl space-y-5">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.settings")}</h2>

      <Section title={t("settings.general")}>
        <div className="mb-2 text-sm font-medium">{t("settings.language")}</div>
        <div className="flex flex-wrap gap-2">
          {LANGUAGES.map((l) => (
            <Button
              key={l.id}
              size="sm"
              variant={language === l.id ? "default" : "outline"}
              onClick={() => setLanguage(l.id)}
            >
              {l.label}
            </Button>
          ))}
        </div>
      </Section>

      <Section title={t("settings.theme")}>
        {kinds.map((kind) => (
          <div key={kind} className="mb-4 last:mb-0">
            <div className="mb-2 text-sm font-medium text-muted-foreground">{t(`settings.themeKinds.${kind}`)}</div>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
              {THEMES.filter((th) => th.kind === kind).map((th) => (
                <button
                  key={th.id}
                  type="button"
                  onClick={() => setTheme(th.id)}
                  className={cn(
                    `theme-${th.id}`,
                    "flex items-center gap-2 rounded-[var(--radius-lg)] border bg-[var(--bg)] p-2 text-left text-sm text-[var(--fg)] transition hover:opacity-90",
                    theme === th.id ? "border-[var(--primary)] ring-2 ring-[var(--primary)]" : "border-[var(--border)]",
                  )}
                >
                  <span className="flex shrink-0 gap-0.5">
                    <span className="h-5 w-3 rounded-sm bg-[var(--primary)]" />
                    <span className="h-5 w-3 rounded-sm bg-[var(--accent)]" />
                    <span className="h-5 w-3 rounded-sm bg-[var(--card)] ring-1 ring-[var(--border)]" />
                  </span>
                  <span className="flex-1 truncate">{t(`settings.themes.${th.id}`)}</span>
                  {theme === th.id && <Check className="h-4 w-4 text-[var(--primary)]" />}
                </button>
              ))}
            </div>
          </div>
        ))}
      </Section>

      <Section title={t("settings.system")}>
        <label className="flex cursor-pointer items-start gap-3">
          <input
            type="checkbox"
            className="mt-1 h-4 w-4 accent-[var(--primary)]"
            checked={autostart.data ?? false}
            disabled={autostart.isPending || toggleAutostart.isPending}
            onChange={(e) => toggleAutostart.mutate(e.target.checked)}
          />
          <span>
            <span className="block text-sm font-medium">{t("settings.autostart")}</span>
            <span className="block text-xs text-muted-foreground">{t("settings.autostartHint")}</span>
          </span>
        </label>
      </Section>

      <Section title={t("settings.rates")}>
        <RatesSection />
      </Section>

      <Section title={t("settings.telegram")}>
        <TgSection />
      </Section>

      <Section title={t("settings.devices")}>
        <SyncSection />
      </Section>

      <Section title={t("settings.security")}>
        <SecuritySection />
      </Section>

      <Section title={t("settings.backup")}>
        <BackupSection />
      </Section>

      <Section title={t("settings.about")}>
        <p className="text-sm text-muted-foreground">
          {t("app.title")} · {t("settings.version")} {info.data?.version ?? "—"}
          {info.data && ` · ${info.data.platform}`}
        </p>
      </Section>
    </div>
  );
}
