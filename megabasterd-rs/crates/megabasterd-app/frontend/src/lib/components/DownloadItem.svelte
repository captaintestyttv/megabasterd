<script lang="ts">
  import type { DownloadInfo } from "../api/tauri";
  import {
    pauseDownload, resumeDownload, cancelDownload, setDownloadSlots, moveDownload
  } from "../api/tauri";
  import { formatBytes, formatSpeed, formatEta, progressPercent } from "../utils/format";

  let { download }: { download: DownloadInfo } = $props();

  let percent = $derived(progressPercent(download.progress, download.file_size));
  let isRunning = $derived(download.state === "Running");
  let isPaused = $derived(download.state === "Paused");
  let isFinished = $derived(download.state === "Finished");

  async function togglePause() {
    if (isRunning) {
      await pauseDownload(download.id);
    } else if (isPaused) {
      await resumeDownload(download.id);
    }
  }
</script>

<div class="flex flex-col gap-1 p-3 border-b border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800">
  <!-- File name and state -->
  <div class="flex justify-between items-center">
    <span class="font-medium text-sm truncate max-w-xs" title={download.file_name}>
      {download.file_name}
    </span>
    <span class="text-xs text-gray-500 ml-2">{download.state}</span>
  </div>

  <!-- Progress bar -->
  <div class="w-full bg-gray-200 dark:bg-gray-700 rounded h-2">
    <div
      class="bg-blue-500 h-2 rounded transition-all duration-300"
      style="width: {percent}%"
    ></div>
  </div>

  <!-- Stats row -->
  <div class="flex justify-between text-xs text-gray-500">
    <span>{formatBytes(download.progress)} / {formatBytes(download.file_size)} ({percent}%)</span>
    <span>{formatSpeed(download.speed)}</span>
    <span>ETA: {formatEta(download.eta_secs)}</span>
  </div>

  <!-- Controls -->
  <div class="flex gap-2 items-center mt-1">
    {#if !isFinished}
      <button
        onclick={togglePause}
        class="px-2 py-1 text-xs rounded bg-blue-500 text-white hover:bg-blue-600 disabled:opacity-50"
      >
        {isRunning ? "Pause" : "Resume"}
      </button>
      <button
        onclick={() => cancelDownload(download.id)}
        class="px-2 py-1 text-xs rounded bg-red-500 text-white hover:bg-red-600"
      >
        Cancel
      </button>

      <!-- Worker slots spinner -->
      <label class="flex items-center gap-1 text-xs">
        Workers:
        <input
          type="number"
          min="1"
          max="20"
          value={download.slots}
          onchange={(e) => setDownloadSlots(download.id, parseInt((e.target as HTMLInputElement).value))}
          class="w-12 border rounded px-1 text-center dark:bg-gray-700"
        />
      </label>
    {/if}

    <!-- Queue priority -->
    <div class="flex gap-1 ml-auto">
      <button onclick={() => moveDownload(download.id, "top")} title="Move to top" class="text-xs px-1">⏫</button>
      <button onclick={() => moveDownload(download.id, "up")} title="Move up" class="text-xs px-1">🔼</button>
      <button onclick={() => moveDownload(download.id, "down")} title="Move down" class="text-xs px-1">🔽</button>
      <button onclick={() => moveDownload(download.id, "bottom")} title="Move to bottom" class="text-xs px-1">⏬</button>
    </div>
  </div>
</div>
