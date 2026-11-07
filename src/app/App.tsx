import { lazy, Suspense, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { useSettingsStore } from "@/stores/settingsStore";
import { PlaceholderPage } from "@/pages/PlaceholderPage";
import { Onboarding } from "./Onboarding";
import { PinLock } from "@/components/PinLock";
import { DbUnlock } from "@/components/DbUnlock";
import { UpdateNotifier } from "@/components/UpdateNotifier";
import { Toaster } from "@/components/ui/toaster";
import { PAGES, type PageId } from "./pages";

const SettingsPage = lazy(() => import("@/pages/SettingsPage").then((m) => ({ default: m.SettingsPage })));
const DashboardPage = lazy(() => import("@/pages/DashboardPage").then((m) => ({ default: m.DashboardPage })));
const AccountsPage = lazy(() => import("@/pages/AccountsPage").then((m) => ({ default: m.AccountsPage })));
const CategoriesPage = lazy(() => import("@/pages/CategoriesPage").then((m) => ({ default: m.CategoriesPage })));
const TransactionsPage = lazy(() => import("@/pages/TransactionsPage").then((m) => ({ default: m.TransactionsPage })));
const JournalPage = lazy(() => import("@/pages/JournalPage").then((m) => ({ default: m.JournalPage })));
const AnalyticsPage = lazy(() => import("@/pages/AnalyticsPage").then((m) => ({ default: m.AnalyticsPage })));
const ReportsPage = lazy(() => import("@/pages/ReportsPage").then((m) => ({ default: m.ReportsPage })));
const RemindersPage = lazy(() => import("@/pages/RemindersPage").then((m) => ({ default: m.RemindersPage })));
const GoalsPage = lazy(() => import("@/pages/GoalsPage").then((m) => ({ default: m.GoalsPage })));

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
        <Suspense
          fallback={
            <div className="flex flex-1 items-center justify-center">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          }
        >
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
        </Suspense>
      </main>
      <UpdateNotifier />
      <Toaster />
    </div>
  );
}
