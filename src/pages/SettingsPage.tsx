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

      <Section title={t("settings.about")}>
        <p className="text-sm text-muted-foreground">
          {t("app.title")} · {t("settings.version")} {info.data?.version ?? "—"}
          {info.data && ` · ${info.data.platform}`}
        </p>
      </Section>
    </div>
  );
}
