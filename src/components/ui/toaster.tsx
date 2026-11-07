import { useEffect } from "react";
import { useToastStore, type Toast } from "@/stores/toast";
import { Button } from "@/components/ui/button";

const TTL_MS = 6000;

function ToastCard({ id, message, actionLabel, onAction }: Toast) {
  const dismiss = useToastStore((s) => s.dismiss);
  useEffect(() => {
    const t = window.setTimeout(() => dismiss(id), TTL_MS);
    return () => window.clearTimeout(t);
  }, [id, dismiss]);
  return (
    <div className="animate-fade-up pointer-events-auto flex items-center gap-3 rounded-xl border border-border bg-card px-4 py-2.5 text-sm shadow-lg">
      <span>{message}</span>
      {actionLabel && (
        <Button
          size="sm"
          variant="ghost"
          className="h-7 px-2 text-primary"
          onClick={() => {
            onAction?.();
            dismiss(id);
          }}
        >
          {actionLabel}
        </Button>
      )}
      <button
        type="button"
        className="text-muted-foreground transition-colors hover:text-foreground"
        onClick={() => dismiss(id)}
        aria-label="×"
      >
        ×
      </button>
    </div>
  );
}

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed bottom-5 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2">
      {toasts.map((t) => (
        <ToastCard key={t.id} {...t} />
      ))}
    </div>
  );
}
