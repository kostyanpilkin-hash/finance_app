import { create } from "zustand";

export interface Toast {
  id: number;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

interface ToastState {
  toasts: Toast[];
  push: (t: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
}

let seq = 1;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  push: (t) => set((s) => ({ toasts: [...s.toasts.slice(-2), { ...t, id: seq++ }] })),
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) })),
}));

/** Show a transient snackbar; `opts.action` renders an undo-style button. */
export function toast(message: string, opts?: { actionLabel?: string; onAction?: () => void }) {
  useToastStore.getState().push({ message, ...opts });
}
