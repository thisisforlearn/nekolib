use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use nekolib::{Blockchain, Config, Miner, Wallet, Transaction, Block, chain::{MAX_SUPPLY, TOKENS_PER_BUMP}};
use nekolib::p2p::{P2PNode, print_nat_guidance};
use std::sync::{Arc, atomic::AtomicBool};
use std::path::Path;

#[derive(Parser)]
#[command(name = "nekod", author = "Vaibhav", version, about = "NekoLib L1 — pure-CPU portable blockchain daemon")]
struct Cli {
    #[arg(short, long, default_value = "nekolib.json")]
    config: String,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long, default_value = "0.0.0.0:9333")]
        listen: String,
        #[arg(long)]
        bootstrap: Option<String>,
    },
    Start {
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        miner_threads: Option<usize>,
    },
    Wallet {
        #[arg(long)]
        show: bool,
    },
    Send {
        to: String,
        amount: u64,
    },
    Info,
    Bench,
    NatGuide,
}

fn banner() {
    println!("{}", r#"
 ███╗   ██╗███████╗██╗  ██╗ ██████╗ ██╗     ██╗██████╗
 ████╗  ██║██╔════╝██║ ██╔╝██╔═══██╗██║     ██║██╔══██╗
 ██╔██╗ ██║█████╗  █████╔╝ ██║   ██║██║     ██║██████╔╝
 ██║╚██╗██║██╔══╝  ██╔═██╗ ██║   ██║██║     ██║██╔══██╗
 ██║ ╚████║███████╗██║  ██╗╚██████╔╝███████╗██║██████╔╝
 ╚═╝  ╚═══╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝╚═════╝"#.bright_magenta().bold());
    println!("{}", format!("  Pure-CPU L1 • BLAKE3 SIMD • Sled • 100k cap • bump every {} tokens", TOKENS_PER_BUMP).bright_black());
    println!("{}", format!("  v{} — by Vaibhav", nekolib::VERSION).bright_black());
    println!();
}

fn print_supply_bar(minted: u64) {
    let pct = (minted as f64 / MAX_SUPPLY as f64 * 100.0).min(100.0);
    let bar_len = 30;
    let filled = ((pct / 100.0) * bar_len as f64) as usize;
    let bar = format!("{}{}", "█".repeat(filled).bright_magenta(), "░".repeat(bar_len - filled).bright_black());
    println!("  Supply {} {}/{} ({:.1}%) remaining {}", bar, minted.to_string().bright_yellow(), MAX_SUPPLY.to_string().bright_white(), pct, (MAX_SUPPLY - minted).to_string().bright_green());
    let bump = minted / TOKENS_PER_BUMP;
    println!("  {} Hardness bumps: {} (next at {} tokens)", "◆".bright_cyan(), bump.to_string().bright_cyan(), ((bump+1)*TOKENS_PER_BUMP).to_string().bright_black());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Start { mine: false, miner_threads: None }) {
        Commands::Init { listen, bootstrap } => {
            banner();
            let mut cfg = Config::default();
            cfg.listen_addr = listen;
            if let Some(b) = bootstrap {
                cfg.bootstrap_peers = vec![b];
            }
            cfg.save(&cli.config)?;
            println!("{} {}", "✓".bright_green(), format!("config written to {}", cli.config).bright_white());
            let _chain = Blockchain::new(&cfg.data_dir, cfg.genesis_difficulty)?;
            println!("{} {}", "✓".bright_green(), format!("genesis created at {} (reward {} per block, cap {} ever)", cfg.data_dir, cfg.block_reward, MAX_SUPPLY).bright_white());
            println!("{}", format!("  edit {} to add bootstrap_peers (DuckDNS/Tailscale)", cli.config).bright_black());
            println!("{}", "  100k cap PERMANENT — no changes can be made after genesis".bright_yellow());
        }
        Commands::Start { mine, miner_threads } => {
            banner();
            let cfg = if Path::new(&cli.config).exists() {
                Config::from_file(&cli.config).unwrap_or_default()
            } else {
                println!("{}", format!("no config at {}, using defaults", cli.config).bright_black());
                Config::default()
            };
            println!("{} {} {}", "●".bright_green(), "NekoLib".bright_white().bold(), format!("v{} — {} (pruning={})", nekolib::VERSION, cfg.listen_addr, cfg.pruning).bright_black());
            println!("  {} {}  {} {}", "data".bright_black(), cfg.data_dir.bright_white(), "diff".bright_black(), cfg.genesis_difficulty.to_string().bright_cyan());
            println!("  {} {}  {} {}", "reward".bright_black(), format!("{} neko/block", cfg.block_reward).bright_yellow(), "cap".bright_black(), format!("{} neko ever", MAX_SUPPLY).bright_magenta());
            if cfg.bootstrap_peers.is_empty() {
                println!("{} {}", "○".bright_yellow(), format!("solo mode — add peers in {} for network", cli.config).bright_black());
            } else {
                println!("{} {:?}", "◉".bright_cyan(), cfg.bootstrap_peers);
            }

            let chain = Blockchain::new(&cfg.data_dir, cfg.genesis_difficulty)?;
            let tip_preview = chain.tip().map(|b| b.hash.chars().take(16).collect::<String>()).unwrap_or_else(|| "none".into());
            let minted = chain.total_minted();
            println!();
            println!("{} {} {} {}", "⛓".bright_magenta(), format!("height {}", chain.height()).bright_white().bold(), format!("hash {}", tip_preview).bright_black(), format!("utxos {}", chain.utxo.len()).bright_black());
            print_supply_bar(minted);
            println!();

            let p2p = P2PNode::new(cfg.clone());
            let chain_arc = Arc::new(std::sync::Mutex::new(chain));
            let chain_for_p2p = Arc::clone(&chain_arc);
            let _listener = p2p.start_listener(move |block| {
                let mut c = chain_for_p2p.lock().unwrap();
                match c.add_block(block.clone()) {
                    Ok(_) => println!("{} {} {} {}", "↓".bright_cyan(), format!("accepted h={}", block.header.height).bright_green(), format!("hash {}", &block.hash[..16]).bright_black(), format!("diff {}", block.header.difficulty).bright_cyan()),
                    Err(e) => eprintln!("{} {}", "✗".bright_red(), format!("rejected block: {}", e).bright_red()),
                }
            })?;
            p2p.bootstrap();
            std::thread::sleep(std::time::Duration::from_millis(300));

            if mine {
                if minted >= MAX_SUPPLY {
                    println!("{}", "■ Cap reached — no more coins can be minted. Mining halted.".bright_red().bold());
                } else {
                    println!("{} {}", "⚡".bright_yellow(), format!("mining with {} threads (BLAKE3 SIMD, AtomicBool) — Enter to stop", miner_threads.unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4))).bright_yellow());
                    let miner = Miner::new(miner_threads);
                    let cancel = Arc::new(AtomicBool::new(false));
                    let chain_mine = Arc::clone(&chain_arc);
                    let cfg_clone = cfg.clone();
                    let cancel_c = Arc::clone(&cancel);
                    // progress spinner for mining
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(ProgressStyle::default_spinner().tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ").template("{spinner:.magenta} {msg}").unwrap());
                    pb.enable_steady_tick(std::time::Duration::from_millis(80));
                    let pb_clone = pb.clone();
                    std::thread::spawn(move || {
                        let mut blocks_mined: u64 = 0;
                        loop {
                            if cancel_c.load(std::sync::atomic::Ordering::Relaxed) { break; }
                            // check cap before building template
                            {
                                let c = chain_mine.lock().unwrap();
                                if c.total_minted() >= MAX_SUPPLY {
                                    pb_clone.set_message(format!("{}", "cap reached — stopping".bright_red()));
                                    break;
                                }
                            }
                            let template = {
                                let c = chain_mine.lock().unwrap();
                                let wallet_path = format!("{}/wallet.json", cfg_clone.data_dir);
                                let addr = if Path::new(&wallet_path).exists() {
                                    let s = std::fs::read_to_string(&wallet_path).unwrap_or_default();
                                    serde_json::from_str::<nekolib::wallet::KeyPair>(&s)
                                        .map(|kp| kp.public_hex)
                                        .unwrap_or_else(|_| Wallet::generate().address())
                                } else {
                                    let w = Wallet::generate();
                                    let _ = std::fs::create_dir_all(&cfg_clone.data_dir);
                                    let _ = std::fs::write(&wallet_path, serde_json::to_string_pretty(&w.keypair).unwrap());
                                    w.address()
                                };
                                let height = c.tip().map(|t| t.header.height + 1).unwrap_or(0);
                                let mut coinbase = Transaction::coinbase(addr.clone(), cfg_clone.block_reward);
                                coinbase.nonce = height;
                                coinbase.payload = Some(format!("coinbase:{}", height));
                                c.next_block_template(vec![coinbase])
                            };
                            let pct = {
                                let c = chain_mine.lock().unwrap();
                                c.total_minted() as f64 / MAX_SUPPLY as f64 * 100.0
                            };
                            pb_clone.set_message(format!("{} {} {} {}",
                                format!("mining h={}", template.header.height).bright_white(),
                                format!("diff={}", template.header.difficulty).bright_cyan(),
                                format!("prev {}...", &template.header.prev_hash[..12]).bright_black(),
                                format!("{:.1}% minted", pct).bright_black()
                            ));
                            let start = std::time::Instant::now();
                            if let Some(mined) = miner.mine(template) {
                                let elapsed = start.elapsed();
                                blocks_mined += 1;
                                pb_clone.println(format!("{} {} {} {} {}",
                                    "✓".bright_green().bold(),
                                    format!("FOUND h={}", mined.header.height).bright_green(),
                                    format!("nonce {}", mined.header.nonce).bright_black(),
                                    format!("hash {}", &mined.hash[..16]).bright_magenta(),
                                    format!("in {:.2}s ({} blocks this session)", elapsed.as_secs_f64(), blocks_mined).bright_black()
                                ));
                                let mut c = chain_mine.lock().unwrap();
                                if let Err(e) = c.add_block(mined.clone()) {
                                    pb_clone.println(format!("{} {}", "✗".bright_red(), e.bright_red()));
                                    if e.contains("cap exceeded") {
                                        pb_clone.set_message(format!("{}", "cap reached".bright_red()));
                                        break;
                                    }
                                } else {
                                    let minted = c.total_minted();
                                    pb_clone.println(format!("  {} {} {}",
                                        format!("⛓ tip h={}", c.height()).bright_white(),
                                        format!("utxos {}", c.utxo.len()).bright_black(),
                                        format!("minted {}/{} (+{} this session)", minted, MAX_SUPPLY, blocks_mined * cfg_clone.block_reward).bright_yellow()
                                    ));
                                }
                                drop(c);
                            } else {
                                pb_clone.println(format!("{}", "cancelled".bright_yellow()));
                                break;
                            }
                            if cancel_c.load(std::sync::atomic::Ordering::Relaxed) { break; }
                            std::thread::sleep(std::time::Duration::from_millis(80));
                        }
                        pb_clone.finish_and_clear();
                    });

                    println!("{}", "  press Enter to stop".bright_black());
                    let mut s = String::new();
                    let _ = std::io::stdin().read_line(&mut s);
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    pb.finish_and_clear();
                    println!("{}", "  stopping...".bright_yellow());
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            } else {
                println!("{} {}", "○".bright_black(), "running without mining. Use --mine to enable.".bright_black());
                println!("{}", "  press Enter to exit".bright_black());
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
            }

            {
                let c = chain_arc.lock().unwrap();
                let _ = c.ledger.flush();
                println!();
                println!("{} {} {}", "■".bright_magenta(), "flushed".bright_white(), format!("{} bytes on disk", c.ledger.size_on_disk()).bright_black());
                print_supply_bar(c.total_minted());
            }
        }
        Commands::Wallet { .. } => {
            banner();
            let w = Wallet::generate();
            println!("{}", "┌──────────────────────────────────────────────┐".bright_magenta());
            println!("{} {}", "│".bright_magenta(), "NEKOLIB WALLET".bright_white().bold());
            println!("{}", "├──────────────────────────────────────────────┤".bright_magenta());
            println!("{} {} {}", "│".bright_magenta(), "address".bright_black(), w.address().bright_cyan().bold());
            println!("{} {} {}", "│".bright_magenta(), "public ".bright_black(), w.keypair.public_hex.bright_white());
            println!("{} {} {}", "│".bright_magenta(), "secret ".bright_black(), w.keypair.secret_hex.bright_yellow());
            println!("{}", "├──────────────────────────────────────────────┤".bright_magenta());
            println!("{} {}", "│".bright_magenta(), "KEEP secret HEX SAFE — anyone with it spends your neko".bright_red());
            println!("{}", "└──────────────────────────────────────────────┘".bright_magenta());
            let cfg = Config::default();
            let path = format!("{}/wallet.json", cfg.data_dir);
            std::fs::create_dir_all(&cfg.data_dir).ok();
            let j = serde_json::to_string_pretty(&w.keypair).unwrap();
            std::fs::write(&path, j).ok();
            println!();
            println!("{} {}", "✓".bright_green(), format!("saved to {}", path).bright_black());
            // show balance if chain exists
            if Path::new(&cfg.data_dir).exists() {
                if let Ok(chain) = Blockchain::new(&cfg.data_dir, cfg.genesis_difficulty) {
                    let bal = chain.get_balance(&w.address());
                    println!("{} {} {}", "●".bright_cyan(), "balance".bright_white(), format!("{} neko ({} utxos total, cap {}/{} minted)", bal, chain.utxo.len(), chain.total_minted(), MAX_SUPPLY).bright_black());
                }
            }
        }
        Commands::Send { to, amount } => {
            banner();
            let cfg = if Path::new(&cli.config).exists() { Config::from_file(&cli.config).unwrap_or_default() } else { Config::default() };
            let chain = Blockchain::new(&cfg.data_dir, cfg.genesis_difficulty)?;
            let wallet_path = format!("{}/wallet.json", cfg.data_dir);
            let kp: nekolib::wallet::KeyPair = if Path::new(&wallet_path).exists() {
                let s = std::fs::read_to_string(&wallet_path).unwrap();
                serde_json::from_str(&s).unwrap()
            } else {
                let w = Wallet::generate();
                println!("{} {}", "○".bright_yellow(), format!("no wallet found, generated new {}", w.address()).bright_black());
                w.keypair
            };
            let mut wallet = Wallet::new(kp);
            let from = wallet.address();
            let bal = chain.get_balance(&from);
            println!("{} {} {}", "●".bright_cyan(), "from".bright_black(), format!("{} (balance {})", &from[..16].bright_white(), bal.to_string().bright_yellow()));
            println!("{} {} {}", "●".bright_cyan(), "to".bright_black(), to.bright_white());
            println!("{} {} {}", "●".bright_cyan(), "amount".bright_black(), amount.to_string().bright_yellow());
            if bal < amount {
                eprintln!("{} {}", "✗".bright_red().bold(), format!("insufficient funds: need {} have {} (cap {}/{})", amount, bal, chain.total_minted(), MAX_SUPPLY).bright_red());
                return Ok(());
            }
            let mut tx = Transaction::new(from.clone(), to.clone(), amount, wallet.nonce);
            wallet.sign_transaction(&mut tx);
            println!();
            println!("{} {}", "✓".bright_green().bold(), format!("tx {} signed", tx.hash().bright_magenta()));
            println!("{}", serde_json::to_string_pretty(&tx).unwrap().bright_black());
            println!();
            println!("{} {}", "→".bright_cyan(), "to broadcast, run node with P2P and AnnounceTx".bright_black());
        }
        Commands::Info => {
            banner();
            let cfg = if Path::new(&cli.config).exists() { Config::from_file(&cli.config).unwrap_or_default() } else { Config::default() };
            let chain = Blockchain::new(&cfg.data_dir, cfg.genesis_difficulty)?;
            let minted = chain.total_minted();
            println!("{} {}", "⛓".bright_magenta().bold(), "CHAIN INFO".bright_white().bold());
            println!("{} {} {}", "─".repeat(50).bright_black(), "".bright_black(), "".bright_black());
            println!("  {} {}", "height".bright_black(), chain.height().to_string().bright_white().bold());
            if let Some(tip) = chain.tip() {
                println!("  {} {}", "hash".bright_black(), tip.hash.bright_magenta());
                println!("  {} {:?}", "header".bright_black(), tip.header);
                println!("  {} {}", "txs in tip".bright_black(), tip.transactions.len().to_string().bright_cyan());
            }
            println!("  {} {} {}", "disk".bright_black(), format!("{} bytes", chain.ledger.size_on_disk()).bright_white(), format!("({:.2} MB)", chain.ledger.size_on_disk() as f64 / 1024.0/1024.0).bright_black());
            println!("  {} {}", "utxos".bright_black(), chain.utxo.len().to_string().bright_cyan());
            println!("  {} {}", "reward".bright_black(), format!("{} neko/block", cfg.block_reward).bright_yellow());
            print_supply_bar(minted);
            println!();
            println!("  {} {}", "top utxos".bright_black(), "(showing 5)".bright_black());
            for (k, u) in chain.utxo.iter().take(5) {
                let kpre: String = k.chars().take(16).collect();
                let tpre: String = u.to.chars().take(16).collect();
                let is_mine = if Path::new(&format!("{}/wallet.json", cfg.data_dir)).exists() {
                    let s = std::fs::read_to_string(format!("{}/wallet.json", cfg.data_dir)).unwrap_or_default();
                    s.contains(&u.to)
                } else { false };
                let mark = if is_mine { "●".bright_green() } else { "○".bright_black() };
                println!("    {} {} {} {}", mark, kpre.bright_black(), "→".bright_black(), format!("{}: {}", tpre.bright_white(), u.amount.to_string().bright_yellow()));
            }
            if minted >= MAX_SUPPLY {
                println!();
                println!("{}", "  ■ CAP REACHED — 100k minted, no more coinbase possible".bright_red().bold());
            } else {
                println!();
                println!("  {} {}", "next difficulty bump".bright_black(), format!("at {} tokens (in {} tokens)", ((minted/TOKENS_PER_BUMP+1)*TOKENS_PER_BUMP), ((minted/TOKENS_PER_BUMP+1)*TOKENS_PER_BUMP - minted)).bright_cyan());
            }
        }
        Commands::Bench => {
            banner();
            println!("{} {}", "⚡".bright_yellow().bold(), "BENCH — BLAKE3 SIMD".bright_white().bold());
            let hps = nekolib::miner::benchmark_hashes(200_000);
            println!("  {} {}", "single-thread".bright_black(), format!("~{} H/s", hps).bright_yellow());
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            println!("  {} {}", format!("{} threads", threads).bright_black(), format!("~{} H/s", hps * threads as u64).bright_green());
            let tx = Transaction::coinbase("bench".into(), 50);
            let block = Block::new("0".repeat(64), 1, vec![tx], 3, 1_700_000_002);
            let miner = Miner::new(None);
            let pb = ProgressBar::new_spinner();
            pb.set_style(ProgressStyle::default_spinner().tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ").template("{spinner:.cyan} {msg}").unwrap());
            pb.set_message("mining demo diff 3...".bright_black().to_string());
            pb.enable_steady_tick(std::time::Duration::from_millis(80));
            let start = std::time::Instant::now();
            let mined = miner.mine(block);
            pb.finish_and_clear();
            if let Some(m) = mined {
                println!("{} {} {} {}", "✓".bright_green().bold(), format!("mined diff 3 in {:.2}s", start.elapsed().as_secs_f64()).bright_white(), format!("nonce {}", m.header.nonce).bright_black(), format!("hash {}", &m.hash[..16]).bright_magenta());
            }
        }
        Commands::NatGuide => {
            banner();
            print_nat_guidance();
        }
    }
    Ok(())
}
