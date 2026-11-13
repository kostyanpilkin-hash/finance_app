import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { NON_DATA_ROOTS } from "./queryKeys";

/** Refetches active db-backed queries when the backend reports a change. */
export function useDataChanged() {
  const qc = useQueryClient();
  useEffect(() => {
    let timer: number | undefined;
    const unlisten = listen<number>("data-changed", () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void qc.invalidateQueries({ predicate: (q) => !NON_DATA_ROOTS.has(String(q.queryKey[0])) });
      }, 300);
    });
    return () => {
      window.clearTimeout(timer);
      void unlisten.then((f) => f());
    };
  }, [qc]);
}
