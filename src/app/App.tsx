import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { useSettingsStore } from "@/stores/settingsStore";
import { PlaceholderPage } from "@/pages/PlaceholderPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { DashboardPage } from "@/pages/DashboardPage";
import { AccountsPage } from "@/pages/AccountsPage";
import { CategoriesPage } from "@/pages/CategoriesPage";
import { TransactionsPage } from "@/pages/TransactionsPage";
import { JournalPage } from "@/pages/JournalPage";
import { AnalyticsPage } from "@/pages/AnalyticsPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { RemindersPage } from "@/pages/RemindersPage";
import { GoalsPage } from "@/pages/GoalsPage";
import { Onboarding } from "./Onboarding";
import { PinLock } from "@/components/PinLock";
import { DbUnlock } from "@/components/DbUnlock";
import { UpdateNotifier } from "@/components/UpdateNotifier";
import { PAGES, type PageId } from "./pages";

export default function App() {
  const { t } = useTranslation();
  const init = useSettingsStore((s) => s.init);
  const [page, setPage] = useState<PageId>("dashboard");

  const dbStatus = useQuery({ queryKey: ["db-status"], queryFn: api.dbStatus });
  const ready = dbStatus.data === "ready";
  const onboarding = useQuery({ queryKey: ["onboarding"], queryFn: api.onboardingStatus, enabled: ready });
  const pinEnabled = useQuery({ queryKey: ["pin-enabled"], queryFn: api.pinEnabled, enabled: ready });
  const [unlockedOnce, setUnlockedOnce] = useState(false);
  const locked = pinEnabled.data === true && !unlockedOnce;

  useEffect(() => void init(), [init]);

  // Auto-lock after 10 minutes of inactivity.
  useEffect(() => {
    if (!pinEnabled.data) return;
    let timer = window.setTimeout(() => setUnlockedOnce(false), 10 * 60 * 1000);
    const reset = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setUnlockedOnce(false), 10 * 60 * 1000);
    };
    const events = ["mousemove", "keydown", "mousedown", "wheel", "touchstart"];
    events.forEach((e) => window.addEventListener(e, reset));
    return () => {
      window.clearTimeout(timer);
      events.forEach((e) => window.removeEventListener(e, reset));
    };
  }, [pinEnabled.data]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const idx = Number(e.key) - 1;
      if (idx >= 0 && idx < 9) {
        e.preventDefault();
        setPage(PAGES[idx].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (dbStatus.data === "locked") {
    return <DbUnlock />;
  }

  if (onboarding.data && !onboarding.data.done) {
    return <Onboarding />;
  }

  if (locked) {
    return (
      <div className="h-screen">
        <PinLock onUnlock={() => setUnlockedOnce(true)} />
      </div>
    );
  }

  return (
    <div className="flex h-screen text-foreground" data-page={page}>
      <aside className="flex w-56 shrink-0 flex-col border-r border-border/80 bg-[var(--sidebar-bg)] p-4 backdrop-blur">
        <h1 className="font-display mb-6 text-2xl font-semibold tracking-tight text-primary">{t("app.title")}</h1>
        <nav className="flex flex-col gap-1">
          {PAGES.map(({ id, icon: Icon }) => (
            <Button
              key={id}
              variant={page === id ? "default" : "ghost"}
              className="justify-start"
              onClick={() => setPage(id)}
            >
              <Icon className="h-4 w-4" />
              {t(`nav.${id}`)}
            </Button>
          ))}
        </nav>
      </aside>

      <main className="flex flex-1 flex-col overflow-auto p-6">
        {page === "settings" ? <SettingsPage />
          : page === "dashboard" ? <DashboardPage />
          : page === "accounts" ? <AccountsPage />
          : page === "categories" ? <CategoriesPage />
          : page === "transactions" ? <TransactionsPage />
          : page === "journal" ? <JournalPage />
          : page === "analytics" ? <AnalyticsPage />
          : page === "reports" ? <ReportsPage />
          : page === "reminders" ? <RemindersPage />
          : page === "goals" ? <GoalsPage />
          : <PlaceholderPage page={page} />}
      </main>
      <UpdateNotifier />
    </div>
  );
}
