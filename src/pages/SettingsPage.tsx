import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { relaunch } from "@tauri-apps/plugin-process";
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
import { qk } from "@/lib/queryKeys";
import { WhatsNewDialog } from "@/components/WhatsNewDialog";
import { THEMES, type ThemeKind } from "@/lib/themes";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUpdateStore } from "@/stores/updateStore";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Section } from "./settings/Section";
import { DashboardCharts, DisplayCurrency, RatesSection } from "./settings/RatesSection";
import { BotNotifySection, TgSection } from "./settings/BotSection";
import { SyncSection } from "./settings/SyncSection";

function SecuritySection() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const enabled = useQuery({ queryKey: qk.pinEnabled, queryFn: api.pinEnabled });
  const [pin, setPin] = useState("");
  const set_ = useMutation({
    mutationFn: () => api.pinSet(pin),
    onSuccess: () => { setPin(""); qc.invalidateQueries({ queryKey: qk.pinEnabled }); },
  });
  const clear = useMutation({
    mutationFn: api.pinClear,
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.pinEnabled }),
  });
  return (
    <div className="space-y-3 text-sm">
      {enabled.data ? (
        <div className="flex items-center gap-3">
          <span>{t("ui.pinOn")}</span>
          <Button size="sm" variant="outline" onClick={() => clear.mutate()}>{t("ui.pinOff")}</Button>
        </div>
      ) : (
        <div className="flex items-end gap-2">
          <Field label={t("ui.pin")} className="w-36">
            <Input type="password" inputMode="numeric"
              maxLength={8} placeholder="4–8" value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))} />
          </Field>
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
  const list = useQuery({ queryKey: qk.backups, queryFn: api.backupList });
  const create = useMutation({
    mutationFn: api.backupCreate,
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.backups }),
  });
  const restore = useMutation({
    mutationFn: api.backupRestore,
    onSuccess: async () => {
      await new Promise((r) => setTimeout(r, 700));
      await relaunch();
    },
  });
  return (
    <div className="space-y-3 text-sm">
      <div className="flex items-center gap-3">
        <Button size="sm" onClick={() => create.mutate()} loading={create.isPending}>{t("ui.backupNow")}</Button>
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
        {restore.isSuccess && <p className="text-xs text-primary">{t("ui.restoreRestarting")}</p>}
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
  const [whatsNewOpen, setWhatsNewOpen] = useState(false);

  const info = useQuery({ queryKey: qk.appInfo, queryFn: api.appInfo });
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
              <div className="mt-4 border-t border-border/50 pt-4">
                <Checkbox
                  checked={autostart.data ?? false}
                  disabled={autostart.isPending || toggleAutostart.isPending}
                  onCheckedChange={(v) => toggleAutostart.mutate(v)}
                  label={
                    <span>
                      <span className="block text-sm font-medium">{t("settings.autostart")}</span>
                      <span className="block text-xs text-muted-foreground">{t("settings.autostartHint")}</span>
                    </span>
                  }
                />
              </div>
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
                <Button variant="outline" onClick={() => setWhatsNewOpen(true)}>{t("whatsnew.show")}</Button>
              </div>
              <WhatsNewDialog open={whatsNewOpen} onClose={() => setWhatsNewOpen(false)} />
            </Section>
          )}
        </div>
      </div>
    </div>
  );
}
