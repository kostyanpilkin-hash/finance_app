import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import {
  Check,
  Coins,
  Database,
  Info,
  MonitorSmartphone,
  Palette,
  Send,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import { LANGUAGES } from "@/i18n";
import { api } from "@/lib/api";
import { THEMES, type ThemeKind } from "@/lib/themes";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUpdateStore } from "@/stores/updateStore";

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
  const status = useQuery({ queryKey: ["tg-status"], queryFn: api.tgStatus, refetchInterval: 15000 });
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
  const stamp = (iso?: string | null) => (iso ? new Date(iso).toLocaleString() : "—");
  const health = s?.health ?? "stopped";
  return (
    <div className="space-y-3 text-sm">
      {s?.enabled && (
        <div className="space-y-1 rounded-xl bg-muted/60 p-3">
          <div className="flex items-center gap-2 font-medium">
            <span className={cn("h-2.5 w-2.5 rounded-full",
              health === "ok" ? "bg-income" : health === "offline" ? "bg-amber-500" : "bg-muted-foreground")} />
            {t(`settings.bot.status.${health}`)}
          </div>
          <div className="text-xs text-muted-foreground">{t("settings.bot.lastMessage")}: {stamp(s.status?.lastMessage)}</div>
          {s.status?.lastError && (
            <div className="text-xs text-destructive">
              {t("settings.bot.lastError")} ({stamp(s.status.lastErrorAt)}): {s.status.lastError}
            </div>
          )}
          {s.logPath && <div className="break-all text-xs text-muted-foreground">{t("settings.bot.log")}: {s.logPath}</div>}
        </div>
      )}
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

function useLocalSettings() {
  const qc = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const set = useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) => api.setSetting(key, value),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const get = (key: string, fallback: string) => settings.data?.[key] || fallback;
  return { get, set: (key: string, value: string) => set.mutate({ key, value }) };
}

const DISPLAY_CURRENCIES = ["BYN", "USD", "EUR"];

function DisplayCurrency() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  const current = get("display_currency", "BYN");
  return (
    <div className="mt-4">
      <div className="mb-2 text-sm font-medium">{t("settings.displayCurrency")}</div>
      <div className="flex flex-wrap gap-2">
        {DISPLAY_CURRENCIES.map((c) => (
          <Button key={c} size="sm" variant={current === c ? "default" : "outline"} onClick={() => set("display_currency", c)}>{c}</Button>
        ))}
      </div>
      <p className="mt-1 text-xs text-muted-foreground">{t("settings.displayCurrencyHint")}</p>
    </div>
  );
}

const DASH_CHARTS: { key: string; label: string }[] = [
  { key: "dash_chart_income_cat", label: "settings.dashChartIncomeCat" },
  { key: "dash_chart_expense_people", label: "settings.dashChartExpensePeople" },
  { key: "dash_chart_income_people", label: "settings.dashChartIncomePeople" },
];

function DashboardCharts() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  return (
    <div className="mt-4 border-t border-border/50 pt-4">
      <div className="mb-2 text-sm font-medium">{t("settings.dashCharts")}</div>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {DASH_CHARTS.map(({ key, label }) => (
          <label key={key} className="flex cursor-pointer items-center gap-2 text-sm font-medium">
            <input
              type="checkbox"
              className="h-4 w-4 accent-[var(--primary)]"
              checked={get(key, "1") === "1"}
              onChange={(e) => set(key, e.target.checked ? "1" : "0")}
            />
            {t(label)}
          </label>
        ))}
      </div>
    </div>
  );
}

function BotNotifySection() {
  const { t } = useTranslation();
  const { get, set } = useLocalSettings();
  const input = "rounded border border-border bg-background px-2 py-1 text-sm";
  const toggle = (key: string, fallback: string, label: string) => (
    <label className="flex min-w-52 cursor-pointer items-center gap-2 font-medium">
      <input type="checkbox" className="h-4 w-4 accent-[var(--primary)]" checked={get(key, fallback) === "1"}
        onChange={(e) => set(key, e.target.checked ? "1" : "0")} />
      {label}
    </label>
  );
  const time = (key: string, fallback: string) => (
    <input type="time" className={input} value={get(key, fallback)}
      onChange={(e) => e.target.value && set(key, e.target.value)} />
  );
  const row = (children: React.ReactNode, hint?: string) => (
    <div className="space-y-1 border-b border-border/50 pb-3 last:border-0">
      <div className="flex flex-wrap items-center gap-3">{children}</div>
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
  return (
    <div className="space-y-3 text-sm">
      {row(<>{toggle("bot_daily_enabled", "1", t("settings.bot.daily"))}<span>{t("settings.bot.at")}</span>{time("bot_daily_time", "23:00")}</>)}
      {row(<>
        {toggle("bot_weekly_enabled", "0", t("settings.bot.weekly"))}
        <select className={input} value={get("bot_weekly_day", "7")} onChange={(e) => set("bot_weekly_day", e.target.value)}>
          {[1, 2, 3, 4, 5, 6, 7].map((d) => <option key={d} value={d}>{t(`ui.wd.${d}`)}</option>)}
        </select>
        <span>{t("settings.bot.at")}</span>{time("bot_weekly_time", "20:00")}
      </>, t("settings.bot.weeklyHint"))}
      {row(<>
        {toggle("bot_monthly_enabled", "0", t("settings.bot.monthly"))}
        <select className={input} value={get("bot_monthly_day", "1")} onChange={(e) => set("bot_monthly_day", e.target.value)}>
          {Array.from({ length: 31 }, (_, i) => i + 1).map((d) => <option key={d} value={d}>{d}</option>)}
        </select>
        <span>{t("settings.bot.day")}</span>
        <span>{t("settings.bot.at")}</span>{time("bot_monthly_time", "20:00")}
      </>, t("settings.bot.monthlyHint"))}
      {row(<>{toggle("bot_reminders_enabled", "1", t("settings.bot.reminders"))}<span>{t("settings.bot.at")}</span>{time("bot_reminders_time", "10:00")}</>,
        t("settings.bot.remindersHint"))}
      {row(<>
        {toggle("bot_quiet_enabled", "0", t("settings.bot.quiet"))}
        <span>{t("settings.bot.from")}</span>{time("bot_quiet_from", "23:30")}
        <span>{t("settings.bot.to")}</span>{time("bot_quiet_to", "08:00")}
      </>, t("settings.bot.quietHint"))}
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
  const addresses = useQuery({ queryKey: ["host-addresses"], queryFn: api.hostAddresses });
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
        {(addresses.data ?? []).length > 0 && (
          <div className="mb-2">
            <div className="text-xs text-muted-foreground">{t("ui.hostAddress")}:</div>
            {(addresses.data ?? []).map((a, i) => (
              <code key={a} className={`block text-sm ${i === 0 ? "font-semibold" : "text-muted-foreground"}`}>{a}</code>
            ))}
          </div>
        )}
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
        {pair.isSuccess && <p className="mt-1 text-xs text-muted-foreground">{t("ui.restoreRestart")}</p>}
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

function UpdateCheckButton() {
  const { t } = useTranslation();
  const { phase, checkNow } = useUpdateStore();
  return (
    <span className="flex items-center gap-2">
      <Button size="sm" variant="outline" onClick={() => void checkNow(true)} disabled={phase === "checking" || phase === "downloading"}>
        {phase === "checking" ? t("update.checking") : t("update.check")}
      </Button>
      {phase === "uptodate" && <span className="text-xs text-income">{t("update.upToDate")}</span>}
    </span>
  );
}

const SECTIONS = [
  { id: "general", icon: SlidersHorizontal },
  { id: "appearance", icon: Palette },
  { id: "rates", icon: Coins },
  { id: "telegram", icon: Send },
  { id: "devices", icon: MonitorSmartphone },
  { id: "security", icon: ShieldCheck },
  { id: "data", icon: Database },
  { id: "about", icon: Info },
] as const;

type SettingsSectionId = (typeof SECTIONS)[number]["id"];

export function SettingsPage() {
  const { t } = useTranslation();
  const { theme, language, setTheme, setLanguage } = useSettingsStore();
  const queryClient = useQueryClient();
  const [section, setSection] = useState<SettingsSectionId>("general");

  const info = useQuery({ queryKey: ["app-info"], queryFn: api.appInfo });
  const autostart = useQuery({ queryKey: ["autostart"], queryFn: isEnabled });
  const toggleAutostart = useMutation({
    mutationFn: (on: boolean) => (on ? enable() : disable()),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["autostart"] }),
  });

  const kinds: ThemeKind[] = ["light", "dark"];

  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl">
      <h2 className="font-display text-2xl font-semibold tracking-tight">{t("nav.settings")}</h2>
      <div className="mt-5 flex items-start gap-6">
        <nav className="flex w-52 shrink-0 flex-col gap-1">
          {SECTIONS.map(({ id, icon: Icon }) => (
            <Button
              key={id}
              size="sm"
              variant={section === id ? "default" : "ghost"}
              className="h-auto min-h-8 w-full px-3 py-2"
              onClick={() => setSection(id)}
            >
              <Icon className="h-4 w-4 shrink-0" />
              <span className="flex-1 truncate text-left">{t(`settings.sections.${id}`)}</span>
            </Button>
          ))}
        </nav>
        <div className="min-w-0 flex-1 space-y-5">
          {section === "general" && (
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
              <DisplayCurrency />
              <DashboardCharts />
              <label className="mt-4 flex cursor-pointer items-start gap-3 border-t border-border/50 pt-4">
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
          )}

          {section === "appearance" && (
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
          )}

          {section === "rates" && (
            <Section title={t("settings.rates")}>
              <RatesSection />
            </Section>
          )}

          {section === "telegram" && (
            <>
              <Section title={t("settings.telegram")}>
                <TgSection />
              </Section>
              <Section title={t("settings.bot.notify")}>
                <BotNotifySection />
              </Section>
            </>
          )}

          {section === "devices" && (
            <Section title={t("settings.devices")}>
              <SyncSection />
            </Section>
          )}

          {section === "security" && (
            <Section title={t("settings.security")}>
              <SecuritySection />
            </Section>
          )}

          {section === "data" && (
            <Section title={t("settings.backup")}>
              <BackupSection />
            </Section>
          )}

          {section === "about" && (
            <Section title={t("settings.about")}>
              <div className="flex items-center gap-3">
                <p className="text-sm text-muted-foreground">
                  {t("app.title")} · {t("settings.version")} {info.data?.version ?? "—"}
                  {info.data && ` · ${info.data.platform}`}
                </p>
                <UpdateCheckButton />
              </div>
            </Section>
          )}
        </div>
      </div>
    </div>
  );
}
