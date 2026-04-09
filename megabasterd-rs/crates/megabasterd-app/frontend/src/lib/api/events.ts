import { listen } from "@tauri-apps/api/event";
import type { DownloadInfo, LinkInfo } from "./tauri";

export const onDownloadProgress = (cb: (info: DownloadInfo) => void) =>
  listen<DownloadInfo>("download-progress", (e) => cb(e.payload));

export const onDownloadFinished = (cb: (id: string) => void) =>
  listen<string>("download-finished", (e) => cb(e.payload));

export const onDownloadError = (cb: (data: { id: string; error: string }) => void) =>
  listen<{ id: string; error: string }>("download-error", (e) => cb(e.payload));

export const onClipboardLinks = (cb: (links: LinkInfo[]) => void) =>
  listen<LinkInfo[]>("clipboard-links", (e) => cb(e.payload));
