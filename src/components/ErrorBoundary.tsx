import { Component, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  fallback: (error: Error, reset: () => void) => ReactNode;
  /** Changing this resets the boundary (e.g. the current page id). */
  resetKey?: unknown;
}

interface State {
  error: Error | null;
}

/** React error boundary: keeps the shell alive when a subtree throws. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    console.error("ErrorBoundary caught", error, info.componentStack);
  }

  componentDidUpdate(prev: Props) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  reset = () => this.setState({ error: null });

  render() {
    if (this.state.error) {
      return this.props.fallback(this.state.error, this.reset);
    }
    return this.props.children;
  }
}

/** Full-screen fallback for a crash outside the page area (main.tsx). */
export function AppCrash({ error }: { error: Error }) {
  const { t } = useTranslation();
  return (
    <div className="grid min-h-screen place-items-center bg-[var(--bg)] p-6">
      <div className="animate-fade-up w-full max-w-lg rounded-2xl border border-border bg-card p-8 shadow-[var(--card-shadow)]">
        <h1 className="font-display mb-1 text-3xl font-semibold text-primary">{t("app.title")}</h1>
        <h2 className="mb-1 font-medium">{t("crash.title")}</h2>
        <p className="mb-4 text-sm text-muted-foreground">{t("crash.hint")}</p>
        <details className="mb-4 rounded-lg bg-muted px-3 py-2 text-xs">
          <summary className="cursor-pointer text-muted-foreground">{t("crash.details")}</summary>
          <pre className="mt-2 whitespace-pre-wrap break-all">{String(error?.message ?? error)}</pre>
        </details>
        <Button className="w-full" onClick={() => window.location.reload()}>
          {t("crash.reload")}
        </Button>
      </div>
    </div>
  );
}

/** Inline fallback for a crashed page — the sidebar stays usable. */
export function PageCrash({ error, reset }: { error: Error; reset: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="animate-fade-up mx-auto w-full max-w-4xl">
      <div className="rounded-2xl border border-border bg-card p-6">
        <h2 className="mb-1 font-medium">{t("crash.pageTitle")}</h2>
        <details className="mb-4 rounded-lg bg-muted px-3 py-2 text-xs">
          <summary className="cursor-pointer text-muted-foreground">{t("crash.details")}</summary>
          <pre className="mt-2 whitespace-pre-wrap break-all">{String(error?.message ?? error)}</pre>
        </details>
        <div className="flex gap-2">
          <Button size="sm" onClick={reset}>{t("crash.retry")}</Button>
          <Button size="sm" variant="outline" onClick={() => window.location.reload()}>
            {t("crash.reload")}
          </Button>
        </div>
      </div>
    </div>
  );
}
