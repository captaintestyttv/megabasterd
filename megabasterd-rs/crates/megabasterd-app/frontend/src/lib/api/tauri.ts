import { invoke } from "@tauri-apps/api/core";

export interface DownloadInfo {
  id: string;
  file_name: string;
  file_size: number;
  progress: number;
  speed: number;
  state: string;
  eta_secs: number | null;
  slots: number;
  url: string;
}

export interface DownloadParams {
  url: string;
  file_id: string;
  file_key: string;
  file_name?: string;
  file_size?: number;
  download_path: string;
  file_pass?: string;
  mega_account_email?: string;
  slots: number;
}

export interface LinkInfo {
  url: string;
  link_type: string;
  file_id?: string;
  key?: string;
}

export interface FolderNode {
  handle: string;
  parent: string;
  name?: string;
  node_type: number;
  size?: number;
  key?: string;
}

export interface AppConfig {
  default_download_path: string;
  max_downloads: number;
  default_slots: number;
  use_slots: boolean;
  dark_mode: boolean;
  monitor_clipboard: boolean;
  use_proxy: boolean;
  proxy_host?: string;
  proxy_port: number;
  use_smart_proxy: boolean;
  smart_proxy_url?: string;
  limit_download_speed: boolean;
  max_download_speed_kbps: number;
  language: string;
}

// Downloads
export const addDownloads = (params: DownloadParams[]) =>
  invoke<string[]>("add_downloads", { params });

export const pauseDownload = (id: string) =>
  invoke<void>("pause_download", { id });

export const resumeDownload = (id: string) =>
  invoke<void>("resume_download", { id });

export const cancelDownload = (id: string) =>
  invoke<void>("cancel_download", { id });

export const setDownloadSlots = (id: string, slots: number) =>
  invoke<void>("set_download_slots", { id, slots });

export const pauseAll = () => invoke<void>("pause_all");
export const resumeAll = () => invoke<void>("resume_all");
export const closeAllFinished = () => invoke<void>("close_all_finished");

export const getDownloads = () => invoke<DownloadInfo[]>("get_downloads");

export const moveDownload = (id: string, direction: "top" | "up" | "down" | "bottom") =>
  invoke<void>("move_download", { id, direction });

// Settings
export const getSettings = () => invoke<AppConfig>("get_settings");
export const saveSettings = (settings: AppConfig) =>
  invoke<void>("save_settings", { settings });
export const selectDirectory = () => invoke<string | null>("select_directory");

// Accounts
export const addMegaAccount = (email: string, password: string) =>
  invoke<void>("add_mega_account", { email, password });
export const removeMegaAccount = (email: string) =>
  invoke<void>("remove_mega_account", { email });
export const getMegaAccounts = () => invoke<string[]>("get_mega_accounts");

// Links
export const detectLinks = (text: string) =>
  invoke<LinkInfo[]>("detect_links", { text });
export const browseFolderLink = (folderId: string, folderKey: string) =>
  invoke<FolderNode[]>("browse_folder_link", { folderId, folderKey });
export const enableClipboardMonitor = (enabled: boolean) =>
  invoke<void>("enable_clipboard_monitor", { enabled });
