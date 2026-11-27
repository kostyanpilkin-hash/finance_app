import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  /** Accessible label for the close button. */
  closeLabel: string;
  children: ReactNode;
  /** Panel width: "md" (default) for short content, "lg" for forms. */
  size?: "md" | "lg";
}

/** Lightweight overlay dialog: closes on Escape and on backdrop click. */
export function Dialog({ open, onClose, title, closeLabel, children, size = "md" }: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  // Portalled to <body>: an animated (transformed) page ancestor would
  // otherwise turn `fixed` into page-relative and misplace the overlay.
  return createPortal(
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/40 p-6"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === "string" ? title : undefined}
        className={`animate-fade-up flex max-h-[80vh] w-full ${size === "lg" ? "max-w-2xl" : "max-w-md"} flex-col rounded-2xl border border-border bg-card p-5 shadow-[var(--card-shadow)]`}
      >
        <div className="mb-3 flex items-center gap-2">
          <h2 className="min-w-0 flex-1 truncate font-medium">{title}</h2>
          <Button size="icon-sm" variant="ghost" aria-label={closeLabel} onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className="min-h-0 overflow-y-auto">{children}</div>
      </div>
    </div>,
    document.body,
  );
}
