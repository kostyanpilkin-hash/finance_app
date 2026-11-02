import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  apiVersion: number;
  platform: string;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  setTrayLabels: (open: string, quit: string, tooltip: string) =>
    invoke<void>("set_tray_labels", { open, quit, tooltip }),
};
