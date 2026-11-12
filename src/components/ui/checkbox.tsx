import * as React from "react";
import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

export interface CheckboxProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type" | "onChange"> {
  /** Label next to the box — the whole 36 px row is clickable. */
  label?: React.ReactNode;
  onCheckedChange?: (checked: boolean) => void;
}

/** 20 px box inside a 36 px clickable row. */
export const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
  ({ className, label, onCheckedChange, checked, defaultChecked, ...props }, ref) => (
    <label
      className={cn(
        "flex min-h-9 cursor-pointer items-center gap-2.5 text-sm text-foreground",
        props.disabled && "cursor-not-allowed opacity-50",
        className,
      )}
    >
      <input
        ref={ref}
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        defaultChecked={defaultChecked}
        onChange={(e) => onCheckedChange?.(e.target.checked)}
        {...props}
      />
      <span
        aria-hidden
        className={cn(
          "flex h-5 w-5 shrink-0 items-center justify-center rounded border border-field-border bg-card text-transparent transition-colors",
          "peer-checked:border-primary peer-checked:bg-primary peer-checked:text-primary-foreground",
          "peer-focus-visible:ring-2 peer-focus-visible:ring-ring peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background",
        )}
      >
        <Check className="size-3.5" strokeWidth={3} />
      </span>
      {label}
    </label>
  ),
);
Checkbox.displayName = "Checkbox";
