use crate::block::Block;
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// P2P message types — JSON line-delimited over pure TCP.
/// No async, no protobuf, minimal overhead for mobile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    Ping { from: String, height: u64 },
    Pong { from: String, height: u64 },
    AnnounceBlock { block: Block },
    RequestChain { from_height: u64 },
    ChainResponse { blocks: Vec<Block> },
    AnnounceTx { tx: crate::block::Transaction },
    Peers { addrs: Vec<String> },
}

impl P2PMessage {
    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap();
        s.push('\n');
        s
    }
    pub fn from_line(line: &str) -> Result<Self, String> {
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())
    }
}

/// Peer manager — tracks connected peers, bootstrap discovery, NAT awareness.
pub struct P2PNode {
    pub config: Config,
    pub peers: Arc<Mutex<HashSet<String>>>,
    pub chain_callback: Option<Arc<Mutex<dyn FnMut(Block) + Send>>>,
}

impl P2PNode {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            peers: Arc::new(Mutex::new(HashSet::new())),
            chain_callback: None,
        }
    }

    pub fn add_peer(&self, addr: String) {
        let mut p = self.peers.lock().unwrap();
        if p.len() < self.config.max_peers {
            p.insert(addr);
        }
    }

    pub fn peer_list(&self) -> Vec<String> {
        self.peers.lock().unwrap().iter().cloned().collect()
    }

    /// Start TCP listener in background thread. Handles inbound connections serially per peer thread.
    /// Pure `std::net::TcpListener`, no Tokio.
    pub fn start_listener(&self, on_block: impl Fn(Block) + Send + 'static) -> std::io::Result<thread::JoinHandle<()>> {
        let addr = self.config.listen_addr.clone();
        let peers = Arc::clone(&self.peers);
        let callback: Arc<Mutex<dyn Fn(Block) + Send>> = Arc::new(Mutex::new(on_block));

        let handle = thread::spawn(move || {
            let listener = match TcpListener::bind(&addr) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[p2p] bind failed {}: {}", addr, e);
                    eprintln!("[p2p] hint: check port forwarding / Tailscale overlay fallback");
                    return;
                }
            };
            println!("[p2p] listening on {}", listener.local_addr().unwrap());
            // Bootstrap after bind
            // incoming loop
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let peer_addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                        {
                            let mut p = peers.lock().unwrap();
                            p.insert(peer_addr.clone());
                        }
                        let cb = Arc::clone(&callback);
                        thread::spawn(move || {
                            handle_connection(stream, cb);
                        });
                    }
                    Err(e) => {
                        eprintln!("[p2p] accept error: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });
        Ok(handle)
    }

    /// Connect to bootstrap peers with resilient DDNS resolution.
    /// Called once at startup; also periodically to re-resolve DuckDNS.
    pub fn bootstrap(&self) {
        for peer in &self.config.bootstrap_peers {
            let peer_c = peer.clone();
            let peers = Arc::clone(&self.peers);
            thread::spawn(move || {
                // Exponential backoff for home internet flaps
                let mut delay = Duration::from_secs(1);
                for attempt in 1..=5 {
                    match peer_c.to_socket_addrs() {
                        Ok(mut addrs) => {
                            if let Some(addr) = addrs.next() {
                                match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                                    Ok(mut stream) => {
                                        println!("[p2p] connected to bootstrap {}", peer_c);
                                        // send ping
                                        let msg = P2PMessage::Ping { from: "self".into(), height: 0 };
                                        let _ = stream.write_all(msg.to_line().as_bytes());
                                        let _ = stream.flush();
                                        peers.lock().unwrap().insert(peer_c.clone());
                                        // keep connection alive briefly for handshake
                                        thread::sleep(Duration::from_secs(2));
                                        return;
                                    }
                                    Err(e) => eprintln!("[p2p] bootstrap connect {} attempt {}: {}", peer_c, attempt, e),
                                }
                            }
                        }
                        Err(e) => eprintln!("[p2p] DNS resolve {}: {}", peer_c, e),
                    }
                    thread::sleep(delay);
                    delay = std::cmp::min(delay * 2, Duration::from_secs(30));
                }
                eprintln!("[p2p] bootstrap {} unreachable after 5 tries", peer_c);
                if peer_c.contains("duckdns") || peer_c.contains("tailscale") || peer_c.contains("100.") {
                    eprintln!("[p2p] hint: ensure DuckDNS is updated (curl https://www.duckdns.org/update?domains=YOUR&token=TOKEN&ip=) or Tailscale is running (tailscale up)");
                }
            });
        }
    }

    /// Broadcast block to all known peers (best-effort, fire-and-forget)
    pub fn broadcast_block(&self, block: &Block) {
        let peers: Vec<String> = self.peer_list();
        let msg_line = P2PMessage::AnnounceBlock { block: block.clone() }.to_line();
        for peer in peers {
            let line = msg_line.clone();
            thread::spawn(move || {
                if let Ok(mut stream) = TcpStream::connect(&peer) {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                    let _ = stream.write_all(line.as_bytes());
                    let _ = stream.flush();
                }
            });
        }
    }

    /// Simple sync: request chain from first bootstrap peer
    pub fn request_sync(&self, from_height: u64) {
        if let Some(peer) = self.config.bootstrap_peers.first().cloned() {
            thread::spawn(move || {
                if let Ok(addrs) = peer.to_socket_addrs().map(|mut i| i.next()) {
                    if let Some(addr) = addrs {
                        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                            let msg = P2PMessage::RequestChain { from_height }.to_line();
                            let _ = stream.write_all(msg.as_bytes());
                            let _ = stream.flush();
                            // wait for response (blocking, with timeout)
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                            let mut reader = BufReader::new(stream);
                            let mut line = String::new();
                            if reader.read_line(&mut line).is_ok() && !line.trim().is_empty() {
                                if let Ok(P2PMessage::ChainResponse { blocks }) = P2PMessage::from_line(&line) {
                                    println!("[p2p] sync got {} blocks", blocks.len());
                                }
                            }
                        }
                    }
                }
            });
        }
    }
}

fn handle_connection(stream: TcpStream, cb: Arc<Mutex<dyn Fn(Block) + Send>>) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".into());
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                if line.trim().is_empty() { continue; }
                match P2PMessage::from_line(&line) {
                    Ok(msg) => match msg {
                        P2PMessage::Ping { from, height } => {
                            println!("[p2p] ping from {} h={}", from, height);
                            // respond pong
                            let resp = P2PMessage::Pong { from: "self".into(), height };
                            let _ = reader.get_mut().write_all(resp.to_line().as_bytes());
                        }
                        P2PMessage::AnnounceBlock { block } => {
                            println!("[p2p] new block {} h={} from {}", block.hash, block.header.height, peer);
                            if let Err(e) = block.verify() {
                                eprintln!("[p2p] invalid block: {}", e);
                            } else {
                                let f = cb.lock().unwrap();
                                f(block);
                            }
                        }
                        P2PMessage::AnnounceTx { tx } => {
                            println!("[p2p] tx {} -> {}", tx.from, tx.to);
                            // mempool insertion would happen here
                        }
                        P2PMessage::RequestChain { from_height } => {
                            println!("[p2p] chain request from {} height {}", peer, from_height);
                            // In real node, we'd load blocks from ledger and respond.
                            // For now, empty response (caller handles via ledger directly)
                            let resp = P2PMessage::ChainResponse { blocks: vec![] };
                            let _ = reader.get_mut().write_all(resp.to_line().as_bytes());
                        }
                        _ => {}
                    },
                    Err(e) => {
                        eprintln!("[p2p] bad message from {}: {} line: {}", peer, e, line.trim());
                        break;
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    // timeout or disconnect
                }
                break;
            }
        }
    }
    println!("[p2p] peer {} disconnected", peer);
}

/// NAT / DDNS helper — prints guidance instead of requiring cloud service.
/// Users map `yourname.duckdns.org` -> home IP via cron: `curl "https://www.duckdns.org/update?domains=YOUR&token=TOKEN&ip="`
/// And Tailscale gives stable 100.x CGNAT-bypassing IPs.
pub fn print_nat_guidance() {
    println!(r#"
[NAT Traversal Guide — Zero Cost]
1. Try port forwarding: router admin -> Forward TCP 9333 -> your LAN IP (e.g., 192.168.1.50)
   Test: `curl ifconfig.me` then `nc -zv YOUR_PUBLIC_IP 9333` from mobile data.

2. If CGNAT / symmetric NAT blocks forwarding:
   Install Tailscale (free tier):
     curl -fsSL https://tailscale.com/install.sh | sh
     sudo tailscale up
     tailscale ip -4  # -> 100.x.y.z  (use this as bootstrap peer)
   All nodes `tailscale up` on same tailnet get mesh VPN; no port forward needed.

3. For dynamic IP, set DuckDNS:
     echo "https://www.duckdns.org/update?domains=YOURNAME&token=YOURTOKEN&ip=" | cron
     bootstrap_peers = ["YOURNAME.duckdns.org:9333", "100.x.y.z:9333"]

4. Local mesh fallback: if no internet, nodes on same WiFi discover via mDNS / manual `bootstrap_peers = ["192.168.1.51:9333"]`
"#);
}
