import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { useSettings } from "@/lib/queries";

export function useLocalSettings() {
  const qc = useQueryClient();
  const settings = useSettings();
  const set = useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) => api.setSetting(key, value),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.settings }),
  });
  const get = (key: string, fallback: string) => settings.data?.[key] || fallback;
  return { get, set: (key: string, value: string) => set.mutate({ key, value }) };
}
