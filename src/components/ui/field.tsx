import * as React from "react";
import { cn } from "@/lib/utils";

interface FieldProps {
  /** Caption above the control — replaces placeholder-only labeling. */
  label?: React.ReactNode;
  htmlFor?: string;
  /** Validation message under the control. */
  error?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}

/** Labeled control wrapper: 12px caption on top, error text below. */
export function Field({ label, htmlFor, error, className, children }: FieldProps) {
  return (
    <div className={cn("space-y-1.5", className)}>
      {label != null && (
        <label htmlFor={htmlFor} className="block text-xs font-medium text-muted-foreground">
          {label}
        </label>
      )}
      {children}
      {error != null && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
