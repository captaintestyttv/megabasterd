<script lang="ts">
  import { onMount } from "svelte";
  import { pauseAll, resumeAll, closeAllFinished } from "./lib/api/tauri";
  import { onClipboardLinks } from "./lib/api/events";
  import { getDownloadList } from "./lib/stores/downloads.svelte";
  import DownloadItem from "./lib/components/DownloadItem.svelte";
  import LinkGrabberDialog from "./lib/components/LinkGrabberDialog.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import type { LinkInfo } from "./lib/api/tauri";

  let showLinkGrabber = $state(false);
  let clipboardLinks = $state<LinkInfo[]>([]);
  let showClipboardPrompt = $state(false);

  // React to clipboard detection
  onMount(async () => {
    const unlisten = await onClipboardLinks((links) => {
      clipboardLinks = links;
      showClipboardPrompt = true;
    });
    return unlisten;
  });

  let downloads = $derived(getDownloadList());
  let running = $derived(downloads.filter(d => d.state === "Running").length);
  let queued = $derived(downloads.filter(d => d.state === "Queued" || d.state === "WaitingToStart").length);
  let finished = $derived(downloads.filter(d => d.state === "Finished").length);
  let globalSpeed = $derived(downloads.reduce((acc, d) => acc + d.speed, 0));
  let totalProgress = $derived(downloads.reduce((acc, d) => acc + d.progress, 0));
  let totalSize = $derived(downloads.reduce((acc, d) => acc + d.file_size, 0));
</script>

<div class="flex flex-col h-screen bg-white dark:bg-gray-950 text-gray-900 dark:text-gray-100 font-sans">
  <!-- Toolbar -->
  <div class="flex items-center gap-2 px-4 py-2 border-b dark:border-gray-700 bg-gray-50 dark:bg-gray-900">
    <span class="font-bold text-blue-600 text-lg mr-4">MegaBasterd</span>

    <button
      onclick={() => showLinkGrabber = true}
      class="px-3 py-1 text-sm bg-blue-500 text-white rounded hover:bg-blue-600"
    >
      + Add Links
    </button>

    <button onclick={() => pauseAll()} class="px-3 py-1 text-sm border rounded hover:bg-gray-100 dark:hover:bg-gray-700">
      Pause All
    </button>
    <button onclick={() => resumeAll()} class="px-3 py-1 text-sm border rounded hover:bg-gray-100 dark:hover:bg-gray-700">
      Resume All
    </button>
    <button onclick={() => closeAllFinished()} class="px-3 py-1 text-sm border rounded hover:bg-gray-100 dark:hover:bg-gray-700">
      Clear Finished
    </button>
  </div>

  <!-- Download list -->
  <div class="flex-1 overflow-auto">
    {#if downloads.length === 0}
      <div class="flex flex-col items-center justify-center h-full text-gray-400 gap-3">
        <p class="text-lg">No downloads</p>
        <button
          onclick={() => showLinkGrabber = true}
          class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
        >
          Add MEGA Links
        </button>
      </div>
    {:else}
      {#each downloads as download (download.id)}
        <DownloadItem {download} />
      {/each}
    {/if}
  </div>

  <!-- Status bar -->
  <StatusBar {globalSpeed} {totalProgress} {totalSize} {running} {queued} {finished} />

  <!-- Modals -->
  {#if showLinkGrabber}
    <LinkGrabberDialog onClose={() => showLinkGrabber = false} />
  {/if}

  <!-- Clipboard prompt -->
  {#if showClipboardPrompt && clipboardLinks.length > 0}
    <div class="fixed bottom-4 right-4 bg-white dark:bg-gray-800 border dark:border-gray-600 rounded-lg shadow-lg p-4 max-w-sm">
      <p class="text-sm font-medium mb-2">
        {clipboardLinks.length} MEGA link(s) detected in clipboard
      </p>
      <div class="flex gap-2">
        <button
          onclick={() => { showLinkGrabber = true; showClipboardPrompt = false; }}
          class="px-3 py-1 text-xs bg-blue-500 text-white rounded hover:bg-blue-600"
        >
          Add to Queue
        </button>
        <button
          onclick={() => showClipboardPrompt = false}
          class="px-3 py-1 text-xs border rounded hover:bg-gray-100 dark:hover:bg-gray-700"
        >
          Dismiss
        </button>
      </div>
    </div>
  {/if}
</div>
