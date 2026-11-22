import { lazy, Suspense, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useDataChanged } from "@/lib/useDataChanged";
import { useSettingsStore } from "@/stores/settingsStore";
import { PlaceholderPage } from "@/pages/PlaceholderPage";
import { Onboarding } from "./Onboarding";
import { PinLock } from "@/components/PinLock";
import { DbUnlock } from "@/components/DbUnlock";
import { DbError } from "@/components/DbError";
import { ErrorBoundary, PageCrash } from "@/components/ErrorBoundary";
import { UpdateNotifier } from "@/components/UpdateNotifier";
import { PairingPrompt } from "@/components/PairingPrompt";
import { WhatsNewAuto } from "@/components/WhatsNewDialog";
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
  useDataChanged();
  const [page, setPage] = useState<PageId>("dashboard");

  const dbStatus = useQuery({ queryKey: ["db-status"], queryFn: api.dbStatus });
  const ready = dbStatus.data === "ready";
  const onboarding = useQuery({ queryKey: qk.onboarding, queryFn: api.onboardingStatus, enabled: ready });
  const pinEnabled = useQuery({ queryKey: qk.pinEnabled, queryFn: api.pinEnabled, enabled: ready });
  const [unlockedOnce, setUnlockedOnce] = useState(false);
  const locked = pinEnabled.data === true && !unlockedOnce;

  useEffect(() => void init(), [init]);

  // macOS WKWebView keeps a stale viewport height after fullscreen or when
  // the window moves to a display with another scale — mirror the real
  // innerHeight into --app-height so .app-frame repaints on every resize.
  useEffect(() => {
    const apply = () =>
      document.documentElement.style.setProperty("--app-height", `${window.innerHeight}px`);
    apply();
    window.addEventListener("resize", apply);
    window.visualViewport?.addEventListener("resize", apply);
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
      getCurrentWindow()
        .onResized(() => apply())
        .then((f) => {
          unlisten = f;
        }),
    );
    return () => {
      window.removeEventListener("resize", apply);
      window.visualViewport?.removeEventListener("resize", apply);
      unlisten?.();
    };
  }, []);

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

  if (dbStatus.data === "error") {
    return (
      <>
        <DbError />
        <UpdateNotifier />
      </>
    );
  }

  if (dbStatus.data === "locked") {
    return (
      <>
        <DbUnlock />
        <UpdateNotifier />
      </>
    );
  }

  if (onboarding.data && !onboarding.data.done) {
    return <Onboarding />;
  }

  if (locked) {
    return (
      <div className="app-frame">
        <PinLock onUnlock={() => setUnlockedOnce(true)} />
      </div>
    );
  }

  return (
    <div className="app-frame flex text-foreground" data-page={page}>
      <aside className="flex w-14 shrink-0 flex-col border-r border-border/80 bg-[var(--sidebar-bg)] p-2 backdrop-blur lg:w-56 lg:p-4">
        <h1 className="font-display mb-6 hidden text-2xl font-semibold tracking-tight text-primary lg:block">{t("app.title")}</h1>
        <div className="font-display mb-6 grid h-10 place-items-center text-xl font-semibold text-primary lg:hidden">Ф</div>
        <nav className="flex flex-col gap-1">
          {PAGES.map(({ id, icon: Icon }) => (
            <Button
              key={id}
              variant={page === id ? "default" : "ghost"}
              className="justify-center lg:justify-start"
              title={t(`nav.${id}`)}
              onClick={() => setPage(id)}
            >
              <Icon className="h-4 w-4 shrink-0" />
              <span className="hidden lg:inline">{t(`nav.${id}`)}</span>
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
        <ErrorBoundary resetKey={page} fallback={(e, reset) => <PageCrash error={e} reset={reset} />}>
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
        </ErrorBoundary>
        </Suspense>
      </main>
      <UpdateNotifier />
      <PairingPrompt />
      <WhatsNewAuto />
      <Toaster />
    </div>
  );
}
