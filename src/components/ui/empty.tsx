import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** Centered empty state: muted icon disc, one line of text, optional hint and CTA. */
export function Empty({
  icon: Icon,
  label,
  hint,
  action,
  className,
}: {
  icon?: LucideIcon;
  label: string;
  hint?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex h-full min-h-28 flex-col items-center justify-center gap-1.5 py-6 text-center",
        className,
      )}
    >
      {Icon && (
        <span className="mb-1 grid h-10 w-10 place-items-center rounded-full bg-muted text-muted-foreground">
          <Icon className="h-5 w-5" />
        </span>
      )}
      <div className="text-sm text-muted-foreground">{label}</div>
      {hint && <div className="max-w-64 text-xs text-muted-foreground/80">{hint}</div>}
      {action}
    </div>
  );
}
