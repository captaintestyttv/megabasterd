<script lang="ts">
  import { formatSpeed, formatBytes } from "../utils/format";

  let { globalSpeed, totalProgress, totalSize, running, queued, finished }: {
    globalSpeed: number;
    totalProgress: number;
    totalSize: number;
    running: number;
    queued: number;
    finished: number;
  } = $props();
</script>

<div class="flex items-center gap-4 px-4 py-1 bg-gray-100 dark:bg-gray-900 border-t dark:border-gray-700 text-xs text-gray-600 dark:text-gray-400">
  <span>⬇ {formatSpeed(globalSpeed)}</span>
  <span>{formatBytes(totalProgress)} / {formatBytes(totalSize)}</span>
  <span class="ml-auto">Running: {running} | Queued: {queued} | Done: {finished}</span>
</div>
