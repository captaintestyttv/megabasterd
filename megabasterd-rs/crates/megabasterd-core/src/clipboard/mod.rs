/// Clipboard monitor, ported from ClipboardSpy.java.
/// Polls clipboard every 250ms and emits MEGA links when detected.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::debug;

use crate::link_parser::{detect_mega_links, MegaLink};

/// CLIPBOARD_CHECK interval in ms (ClipboardSpy.java SLEEP = 250)
const SLEEP_MS: u64 = 250;

pub struct ClipboardMonitor {
    enabled: AtomicBool,
}

impl ClipboardMonitor {
    pub fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(enabled),
        })
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Run the polling loop. Sends detected links to the subscriber channel.
    pub async fn run(self: Arc<Self>, sender: mpsc::Sender<Vec<MegaLink>>) {
        let mut last_content = String::new();

        loop {
            sleep(Duration::from_millis(SLEEP_MS)).await;

            if !self.is_enabled() {
                continue;
            }

            let content = match Self::read_clipboard() {
                Some(c) => c,
                None => continue,
            };

            if content == last_content {
                continue;
            }
            last_content = content.clone();

            let links = detect_mega_links(&content);
            if !links.is_empty() {
                debug!("Clipboard: detected {} MEGA link(s)", links.len());
                let _ = sender.send(links).await;
            }
        }
    }

    fn read_clipboard() -> Option<String> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        clipboard.get_text().ok()
    }
}
