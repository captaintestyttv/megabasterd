import { onDownloadProgress, onDownloadFinished, onDownloadError } from "../api/events";
import { getDownloads, type DownloadInfo } from "../api/tauri";

// Svelte 5 reactive state
let downloads = $state<Record<string, DownloadInfo>>({});

// Subscribe to backend events
onDownloadProgress((info) => {
  downloads[info.id] = info;
});

onDownloadFinished((id) => {
  if (downloads[id]) {
    downloads[id].state = "Finished";
  }
});

onDownloadError(({ id, error }) => {
  if (downloads[id]) {
    downloads[id].state = `Failed: ${error}`;
  }
});

// Initial load
getDownloads().then((list) => {
  for (const d of list) {
    downloads[d.id] = d;
  }
});

export function getDownloadList(): DownloadInfo[] {
  return Object.values(downloads);
}

export function getDownload(id: string): DownloadInfo | undefined {
  return downloads[id];
}

export function removeDownload(id: string) {
  delete downloads[id];
}
