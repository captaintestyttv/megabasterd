<script lang="ts">
  import { detectLinks, addDownloads, selectDirectory, type LinkInfo } from "../api/tauri";

  let { onClose }: { onClose: () => void } = $props();

  let pastedText = $state("");
  let detectedLinks = $state<LinkInfo[]>([]);
  let downloadPath = $state("");
  let detecting = $state(false);

  async function detect() {
    detecting = true;
    try {
      detectedLinks = await detectLinks(pastedText);
    } finally {
      detecting = false;
    }
  }

  async function chooseDir() {
    const dir = await selectDirectory();
    if (dir) downloadPath = dir;
  }

  async function startDownloads() {
    if (!downloadPath) {
      alert("Please select a download folder first.");
      return;
    }
    const fileLinks = detectedLinks.filter(l => l.link_type === "file" && l.file_id && l.key);
    if (fileLinks.length === 0) return;

    await addDownloads(fileLinks.map(l => ({
      url: l.url,
      file_id: l.file_id!,
      file_key: l.key!,
      download_path: downloadPath,
      slots: 6,
    })));
    onClose();
  }
</script>

<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
  <div class="bg-white dark:bg-gray-900 rounded-lg shadow-xl w-[560px] max-h-[80vh] flex flex-col">
    <div class="flex justify-between items-center p-4 border-b dark:border-gray-700">
      <h2 class="font-semibold text-lg">Add Downloads</h2>
      <button onclick={onClose} class="text-gray-400 hover:text-gray-600">✕</button>
    </div>

    <div class="p-4 flex flex-col gap-3 overflow-auto">
      <textarea
        bind:value={pastedText}
        placeholder="Paste MEGA links here (one per line or mixed with text)..."
        class="w-full h-32 border rounded p-2 text-sm font-mono dark:bg-gray-800 dark:border-gray-700 resize-none"
      ></textarea>

      <button
        onclick={detect}
        disabled={detecting || !pastedText.trim()}
        class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 self-start"
      >
        {detecting ? "Detecting..." : "Detect Links"}
      </button>

      {#if detectedLinks.length > 0}
        <div class="border rounded dark:border-gray-700">
          <div class="p-2 text-sm font-medium border-b dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
            {detectedLinks.length} link(s) detected
          </div>
          <ul class="max-h-40 overflow-auto">
            {#each detectedLinks as link}
              <li class="p-2 text-xs border-b dark:border-gray-700 truncate" title={link.url}>
                <span class="text-blue-500">[{link.link_type}]</span> {link.url}
              </li>
            {/each}
          </ul>
        </div>

        <div class="flex gap-2 items-center">
          <input
            type="text"
            bind:value={downloadPath}
            placeholder="Download folder..."
            class="flex-1 border rounded px-2 py-1 text-sm dark:bg-gray-800 dark:border-gray-700"
            readonly
          />
          <button onclick={chooseDir} class="px-3 py-1 text-sm border rounded hover:bg-gray-100 dark:hover:bg-gray-700">
            Browse
          </button>
        </div>
      {/if}
    </div>

    <div class="flex justify-end gap-2 p-4 border-t dark:border-gray-700">
      <button onclick={onClose} class="px-4 py-2 text-sm border rounded hover:bg-gray-100 dark:hover:bg-gray-700">
        Cancel
      </button>
      <button
        onclick={startDownloads}
        disabled={detectedLinks.length === 0 || !downloadPath}
        class="px-4 py-2 text-sm bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50"
      >
        Download ({detectedLinks.filter(l => l.link_type === "file").length})
      </button>
    </div>
  </div>
</div>
