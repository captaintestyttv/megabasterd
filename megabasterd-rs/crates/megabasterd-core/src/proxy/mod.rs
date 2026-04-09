/// Smart proxy manager, ported from SmartMegaProxyManager.java.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time;
use tracing::{info, warn};

use crate::config::{
    SMART_PROXY_AUTOREFRESH_MIN, SMART_PROXY_BAN_TIME_S, SMART_PROXY_TIMEOUT_MS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyType {
    Http,
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    pub fn url(&self) -> String {
        let scheme = match self.proxy_type {
            ProxyType::Http => "http",
            ProxyType::Socks5 => "socks5",
        };
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            format!("{}://{}:{}@{}:{}", scheme, user, pass, self.host, self.port)
        } else {
            format!("{}://{}:{}", scheme, self.host, self.port)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartProxyConfig {
    pub proxy_list_url: Option<String>,
    pub ban_time_secs: u32,
    pub timeout_ms: u32,
    pub autorefresh_min: u32,
    pub random_select: bool,
    pub reset_slot_proxy: bool,
    pub force_smart_proxy: bool,
}

impl Default for SmartProxyConfig {
    fn default() -> Self {
        Self {
            proxy_list_url: None,
            ban_time_secs: SMART_PROXY_BAN_TIME_S,
            timeout_ms: SMART_PROXY_TIMEOUT_MS,
            autorefresh_min: SMART_PROXY_AUTOREFRESH_MIN,
            random_select: true,
            reset_slot_proxy: true,
            force_smart_proxy: false,
        }
    }
}

#[derive(Debug)]
struct ProxyEntry {
    address: String,
    proxy_type: ProxyType,
    blocked_until: Option<Instant>,
}

impl ProxyEntry {
    fn is_blocked(&self) -> bool {
        self.blocked_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }
}

pub struct SmartProxyManager {
    proxies: RwLock<Vec<ProxyEntry>>,
    pub config: SmartProxyConfig,
}

impl SmartProxyManager {
    pub fn new(config: SmartProxyConfig) -> Self {
        Self {
            proxies: RwLock::new(Vec::new()),
            config,
        }
    }

    /// Fetch the proxy list from the configured URL and populate internal list.
    pub async fn refresh_proxy_list(&self) -> Result<()> {
        let url = match &self.config.proxy_list_url {
            Some(u) => u.clone(),
            None => return Ok(()),
        };

        let resp = reqwest::get(&url).await?.text().await?;
        let parsed: serde_json::Value = serde_json::from_str(&resp)?;

        let mut proxies = self.proxies.write().await;
        proxies.clear();

        // Expected format: array of {"proxy": "host:port", "type": "http"|"socks"}
        if let Some(arr) = parsed.as_array() {
            for item in arr {
                let address = item.get("proxy")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let proxy_type = if item.get("type").and_then(|v| v.as_str()) == Some("socks") {
                    ProxyType::Socks5
                } else {
                    ProxyType::Http
                };
                if !address.is_empty() {
                    proxies.push(ProxyEntry {
                        address,
                        proxy_type,
                        blocked_until: None,
                    });
                }
            }
        }

        info!("Loaded {} proxies from {}", proxies.len(), url);
        Ok(())
    }

    /// Get an available proxy, excluding specified addresses.
    /// Returns (address, proxy_type) or None if no proxies are available.
    pub async fn get_proxy(&self, excluded: &[String]) -> Option<(String, ProxyType)> {
        let proxies = self.proxies.read().await;
        let available: Vec<&ProxyEntry> = proxies
            .iter()
            .filter(|p| !p.is_blocked() && !excluded.contains(&p.address))
            .collect();

        if available.is_empty() {
            return None;
        }

        if self.config.random_select {
            let mut rng = rand::thread_rng();
            available.choose(&mut rng).map(|p| (p.address.clone(), p.proxy_type.clone()))
        } else {
            available.first().map(|p| (p.address.clone(), p.proxy_type.clone()))
        }
    }

    /// Block a proxy for the configured ban duration.
    pub async fn block_proxy(&self, address: &str, reason: &str) {
        let mut proxies = self.proxies.write().await;
        if let Some(entry) = proxies.iter_mut().find(|p| p.address == address) {
            let ban_duration = Duration::from_secs(self.config.ban_time_secs as u64);
            entry.blocked_until = Some(Instant::now() + ban_duration);
            warn!("Blocked proxy {} for {}s (reason: {})", address, self.config.ban_time_secs, reason);
        }
    }

    pub async fn count_blocked(&self) -> usize {
        self.proxies.read().await.iter().filter(|p| p.is_blocked()).count()
    }

    pub async fn count_available(&self) -> usize {
        self.proxies.read().await.iter().filter(|p| !p.is_blocked()).count()
    }

    pub async fn count_total(&self) -> usize {
        self.proxies.read().await.len()
    }

    /// Start the auto-refresh background task.
    pub async fn start_auto_refresh(self: Arc<Self>) {
        let interval = Duration::from_secs(self.config.autorefresh_min as u64 * 60);
        // Initial load
        if let Err(e) = self.refresh_proxy_list().await {
            warn!("Initial proxy list load failed: {}", e);
        }
        loop {
            time::sleep(interval).await;
            if let Err(e) = self.refresh_proxy_list().await {
                warn!("Proxy list auto-refresh failed: {}", e);
            }
        }
    }

    /// Build a reqwest::Proxy from a proxy address and type.
    pub fn build_reqwest_proxy(address: &str, proxy_type: &ProxyType) -> Result<reqwest::Proxy> {
        let scheme = match proxy_type {
            ProxyType::Http => "http",
            ProxyType::Socks5 => "socks5",
        };
        let url = format!("{}://{}", scheme, address);
        Ok(reqwest::Proxy::all(&url)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> SmartProxyManager {
        SmartProxyManager::new(SmartProxyConfig {
            ban_time_secs: 300,
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn test_block_and_unblock() {
        let mgr = make_manager();
        // Manually add a proxy entry
        {
            let mut proxies = mgr.proxies.write().await;
            proxies.push(ProxyEntry {
                address: "1.2.3.4:8080".to_string(),
                proxy_type: ProxyType::Http,
                blocked_until: None,
            });
        }
        assert_eq!(mgr.count_available().await, 1);
        mgr.block_proxy("1.2.3.4:8080", "test").await;
        assert_eq!(mgr.count_available().await, 0);
        assert_eq!(mgr.count_blocked().await, 1);
    }

    #[tokio::test]
    async fn test_get_proxy_excludes() {
        let mgr = make_manager();
        {
            let mut proxies = mgr.proxies.write().await;
            proxies.push(ProxyEntry {
                address: "1.2.3.4:8080".to_string(),
                proxy_type: ProxyType::Http,
                blocked_until: None,
            });
            proxies.push(ProxyEntry {
                address: "5.6.7.8:3128".to_string(),
                proxy_type: ProxyType::Http,
                blocked_until: None,
            });
        }
        let proxy = mgr.get_proxy(&["1.2.3.4:8080".to_string()]).await;
        assert_eq!(proxy.map(|(a, _)| a), Some("5.6.7.8:3128".to_string()));
    }

    #[tokio::test]
    async fn test_get_proxy_none_when_all_blocked() {
        let mgr = make_manager();
        {
            let mut proxies = mgr.proxies.write().await;
            proxies.push(ProxyEntry {
                address: "1.2.3.4:8080".to_string(),
                proxy_type: ProxyType::Http,
                blocked_until: Some(Instant::now() + Duration::from_secs(300)),
            });
        }
        let proxy = mgr.get_proxy(&[]).await;
        assert!(proxy.is_none());
    }
}
