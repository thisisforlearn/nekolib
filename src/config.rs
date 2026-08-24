use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Node configuration — zero-cloud, file-driven or env.
/// Bootstrap + DDNS + Tailscale overlay support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// This node's listen address (e.g., "0.0.0.0:9333")
    pub listen_addr: String,
    /// Hardcoded or config-driven bootstrap peers (DNS names allowed for DDNS)
    /// Example: ["neko-seed.duckdns.org:9333", "100.64.0.5:9333"] (Tailscale IP)
    pub bootstrap_peers: Vec<String>,
    /// Node name for debugging
    pub node_name: String,
    /// Data directory for sled
    pub data_dir: String,
    /// Initial difficulty (1 = easiest, 6 = ~1M hashes)
    pub genesis_difficulty: u32,
    /// Enable pruning (mobile: true, server: false)
    pub pruning: bool,
    /// Enable Tailscale overlay fallback if NAT traversal fails
    pub tailscale_fallback: bool,
    /// Max peers
    pub max_peers: usize,
    /// Block reward — FIXED at genesis. Do not change after launch or chain forks.
    pub block_reward: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9333".to_string(),
            bootstrap_peers: vec![
                // Replace with your DuckDNS / Tailscale IP
                // "your-name.duckdns.org:9333".to_string(),
            ],
            node_name: "neko-node".to_string(),
            data_dir: "./nekodata".to_string(),
            genesis_difficulty: 3,
            pruning: true,
            tailscale_fallback: true,
            max_peers: 32,
            block_reward: 50, // 50 per block, 100k cap = 2000 blocks total — permanent
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, content).map_err(|e| e.to_string())
    }

    pub fn listen_socket(&self) -> Result<SocketAddr, String> {
        self.listen_addr.parse().map_err(|e: std::net::AddrParseError| e.to_string())
    }

    /// Resolve bootstrap peers (DNS -> IP, handles dynamic DNS)
    pub fn resolved_bootstrap(&self) -> Vec<SocketAddr> {
        let mut out = Vec::new();
        for peer in &self.bootstrap_peers {
            if let Ok(addrs) = peer.to_socket_addrs_resilient() {
                out.extend(addrs);
            }
        }
        out
    }
}

trait ToSocketAddrsResilient {
    fn to_socket_addrs_resilient(&self) -> std::io::Result<Vec<SocketAddr>>;
}

impl ToSocketAddrsResilient for String {
    fn to_socket_addrs_resilient(&self) -> std::io::Result<Vec<SocketAddr>> {
        use std::net::ToSocketAddrs;
        let addrs: Vec<SocketAddr> = self.to_socket_addrs()?.collect();
        Ok(addrs)
    }
}
