use crate::node::{Node, NodeConfig};
use crate::rpc::start_rpc;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use clap::{Parser, Subcommand};
use futures::{StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use libp2p::Multiaddr;
use reqwest::Body;
use rpassword::prompt_password;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio_util::io::ReaderStream;
use tracing_subscriber::EnvFilter;

fn normalize_rpc_endpoint(raw: &str) -> String {
    if raw.starts_with(':') {
        return format!("http://127.0.0.1{}", raw);
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("http://{}", raw)
    }
}

fn parse_bootnode_str(raw: &str) -> Option<Multiaddr> {
    if let Ok(ma) = raw.parse() {
        return Some(ma);
    }

    if !raw.contains("/p2p/") {
        if let Some((prefix, peer)) = raw.rsplit_once('/') {
            if peer.len() >= 40 {
                let candidate = format!("{}/p2p/{}", prefix, peer);
                if let Ok(ma) = candidate.parse() {
                    return Some(ma);
                }
            }
        }
    }
    None
}

fn prompt_for_passphrase() -> Result<String> {
    let pass = prompt_password("Identity passphrase: ").context("reading passphrase")?;
    if pass.is_empty() {
        return Err(anyhow::anyhow!("passphrase cannot be empty"));
    }
    let confirm = prompt_password("Confirm passphrase: ").context("reading passphrase")?;
    if pass != confirm {
        return Err(anyhow::anyhow!("passphrases do not match"));
    }
    Ok(pass)
}

fn prompt_for_passphrase_once() -> Result<String> {
    let pass = prompt_password("Identity passphrase: ").context("reading passphrase")?;
    if pass.is_empty() {
        return Err(anyhow::anyhow!("passphrase cannot be empty"));
    }
    Ok(pass)
}

#[derive(Parser)]
#[command(author, version, about = "ONVM CLI powered by RPC")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a data directory with node keys
    Init {
        #[arg(long, default_value = "./data")]
        data_dir: PathBuf,
        #[arg(long, help = "Encrypt identity key with this passphrase")]
        identity_passphrase: Option<String>,
        #[arg(
            long,
            default_value_t = false,
            help = "Permit plaintext identity storage (insecure)"
        )]
        allow_plaintext_identity: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Enable mDNS in generated config"
        )]
        enable_mdns: bool,
        #[arg(long, help = "Override min peers in generated config")]
        min_peers: Option<usize>,
        #[arg(long, help = "Override max gossip bytes in generated config")]
        max_gossip_bytes: Option<usize>,
        #[arg(long, help = "Override max inbound connections in generated config")]
        max_inbound_connections: Option<usize>,
        #[arg(long, help = "Override max inbound streams in generated config")]
        max_inbound_streams: Option<usize>,
        #[arg(long, help = "Disable required encryption in generated config")]
        allow_insecure_transport: bool,
        #[arg(long, help = "Set genesis state root (hex) in generated config")]
        genesis_state_root: Option<String>,
    },
    /// Run a full ONVM node (network + consensus + RPC)
    RunNode {
        #[arg(long, default_value = "./data")]
        data_dir: PathBuf,
        #[arg(long, default_value = "/ip4/0.0.0.0/tcp/37000")]
        listen: String,
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long, default_value_t = 1)]
        min_peers: usize,
        #[arg(long, default_value = "full", value_parser = ["full", "metadata"], help = "Blob sync mode: full data replication or metadata-only")]
        blob_sync_mode: String,
        #[arg(
            long,
            default_value = "config.toml",
            help = "Path to config.toml (relative to data dir)"
        )]
        config: String,
        #[arg(
            long,
            num_args = 0..,
            value_delimiter = ',',
            help = "Bootstrap peer multiaddrs (comma-separated or repeated)"
        )]
        bootnode: Vec<String>,
        #[arg(long, help = "Passphrase to decrypt identity key")]
        identity_passphrase: Option<String>,
        #[arg(
            long,
            default_value_t = false,
            help = "Permit plaintext identity storage (insecure)"
        )]
        allow_plaintext_identity: bool,
        #[arg(long, help = "Development mode: enable mDNS regardless of config")]
        dev: bool,
    },
    /// Submit a job to a running node
    SubmitJob {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        request_id: Option<String>,
        #[arg(long, default_value_t = 3)]
        max_retries: u32,
    },
    /// Get job status
    JobStatus {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        job_id: String,
    },
    /// Get job logs
    JobLogs {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        job_id: String,
    },
    /// Get job output
    JobOutput {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        job_id: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Cancel a job
    CancelJob {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        job_id: String,
    },
    /// List all jobs
    ListJobs {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
    },
    /// Upload a blob to a running node
    UploadBlob {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        mime: Option<String>,
    },
    /// Deploy a WASM program
    Deploy {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        entrypoint: String,
        #[arg(long, num_args = 0..)]
        blob_refs: Vec<String>,
        #[arg(
            long,
            help = "Base64 salt for unique ProgramId; defaults to random",
            alias = "salt-base64"
        )]
        salt: Option<String>,
    },
    /// Execute a program
    Execute {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(
            long,
            help = "Print raw JSON response instead of base64-decoded stdout"
        )]
        json: bool,
        #[arg(short = 'e', long, help = "Estimate fuel cost without executing")]
        estimate: bool,
    },
    /// Fetch a blob
    GetBlob {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Inspect a program
    ProgramInfo {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        id: String,
    },
}

pub async fn run() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,libp2p_mdns=off"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Init {
            data_dir,
            identity_passphrase,
            allow_plaintext_identity,
            enable_mdns,
            min_peers,
            max_gossip_bytes,
            max_inbound_connections,
            max_inbound_streams,
            allow_insecure_transport,
            genesis_state_root,
        } => {
            let identity_path = data_dir.join("identity");
            tokio::fs::create_dir_all(&data_dir).await?;

            if identity_path.exists() {
                println!(
                    "Identity already exists at {}. Skipping generation.",
                    identity_path.display()
                );
            } else {
                let identity_passphrase =
                    if identity_passphrase.is_none() && !allow_plaintext_identity {
                        Some(prompt_for_passphrase()?)
                    } else {
                        identity_passphrase
                    };
                let keys = crate::crypto::keys::NodeKeys::load_or_generate(
                    &identity_path,
                    identity_passphrase.as_deref(),
                    allow_plaintext_identity,
                )
                .await?;
                println!(
                    "Initialized identity at {}. Node ID: {}",
                    identity_path.display(),
                    keys.node_id
                );
            }

            let cfg_path = data_dir.join("config.toml");
            let overrides_present = enable_mdns
                || min_peers.is_some()
                || max_gossip_bytes.is_some()
                || max_inbound_connections.is_some()
                || max_inbound_streams.is_some()
                || allow_insecure_transport
                || genesis_state_root.is_some();

            if !cfg_path.exists() && !overrides_present {
                crate::config::OnvmConfig::write_to(&cfg_path)?;
                println!("Wrote default config to {}", cfg_path.display());
            } else if cfg_path.exists() || overrides_present {
                let mut cfg = if cfg_path.exists() {
                    crate::config::OnvmConfig::from_file(&cfg_path).unwrap_or_default()
                } else {
                    crate::config::OnvmConfig::default()
                };

                if enable_mdns {
                    cfg.network.enable_mdns = true;
                }
                if let Some(v) = min_peers {
                    cfg.network.min_peers = v;
                }
                if let Some(v) = max_gossip_bytes {
                    cfg.network.max_gossip_bytes = v;
                }
                if let Some(v) = max_inbound_connections {
                    cfg.network.max_inbound_connections = v;
                }
                if let Some(v) = max_inbound_streams {
                    cfg.network.max_inbound_streams = v;
                }
                if allow_insecure_transport {
                    cfg.network.require_encryption = false;
                }
                if let Some(root_hex) = genesis_state_root {
                    if root_hex.is_empty() {
                        cfg.genesis.state_root = None;
                    } else if let Ok(bytes) = hex::decode(&root_hex) {
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            cfg.genesis.state_root = Some(arr);
                        } else {
                            println!("Ignoring genesis_state_root override: expected 32-byte hex");
                        }
                    } else {
                        println!("Ignoring genesis_state_root override: invalid hex");
                    }
                }

                let content = cfg.to_toml();
                let text = toml::to_string_pretty(&content)?;
                std::fs::write(&cfg_path, text)?;
                println!("Config written to {}", cfg_path.display());
            }
        }
        Commands::RunNode {
            data_dir,
            listen,
            rpc,
            min_peers,
            blob_sync_mode,
            config,
            bootnode,
            identity_passphrase,
            allow_plaintext_identity,
            dev,
        } => {
            let identity_path = data_dir.join("identity");
            let listen_addr: Multiaddr = listen
                .parse()
                .with_context(|| format!("invalid listen multiaddr {listen}"))?;
            let rpc_endpoint = normalize_rpc_endpoint(&rpc);
            let rpc_addr: SocketAddr = rpc_endpoint
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .parse()?;
            let identity_passphrase = if identity_passphrase.is_none() && !allow_plaintext_identity
            {
                Some(prompt_for_passphrase_once()?)
            } else {
                identity_passphrase
            };
            tokio::fs::create_dir_all(&data_dir).await?;
            if !identity_path.exists() {
                return Err(anyhow::anyhow!(
                    "identity not found at {}; run `init` first to create one",
                    identity_path.display()
                ));
            }
            let identity = Arc::new(
                crate::crypto::keys::NodeKeys::load_or_generate(
                    &identity_path,
                    identity_passphrase.as_deref(),
                    allow_plaintext_identity,
                )
                .await?,
            );
            let cfg_path = data_dir.join(&config);
            let onvm_cfg = if cfg_path.exists() {
                crate::config::OnvmConfig::from_file(&cfg_path).unwrap_or_else(|e| {
                    tracing::warn!("failed to parse config.toml, using defaults: {}", e);
                    crate::config::OnvmConfig::default()
                })
            } else {
                crate::config::OnvmConfig::default()
            };
            let mut onvm_cfg = onvm_cfg;
            if dev {
                onvm_cfg.network.enable_mdns = true;
            }
            let bootnodes = if !bootnode.is_empty() {
                bootnode
            } else {
                onvm_cfg.network.bootnodes.clone()
            };
            let parsed_bootnodes: Vec<Multiaddr> = bootnodes
                .iter()
                .filter_map(|addr| match parse_bootnode_str(addr) {
                    Some(ma) => Some(ma),
                    None => {
                        tracing::warn!("skipping invalid bootnode {addr}");
                        None
                    }
                })
                .collect();
            let blob_mode = if blob_sync_mode == "metadata" {
                crate::consensus::BlobSyncMode::MetadataOnly
            } else {
                crate::consensus::BlobSyncMode::FullData
            };
            let node = Arc::new(
                Node::start(NodeConfig {
                    data_dir: data_dir.clone(),
                    listen_addr,
                    rpc_bind: rpc_addr,
                    min_peers,
                    blob_sync_mode: blob_mode,
                    identity: (*identity).clone(),
                    network: crate::node::NetworkSecurityConfig {
                        enable_mdns: onvm_cfg.network.enable_mdns,
                        require_encryption: onvm_cfg.network.require_encryption,
                        max_inbound_connections: onvm_cfg.network.max_inbound_connections,
                        max_inbound_streams: onvm_cfg.network.max_inbound_streams,
                        max_gossip_bytes: onvm_cfg.network.max_gossip_bytes,
                    },
                    bootnodes: parsed_bootnodes,
                })
                .await?,
            );
            let rpc_server = start_rpc(node.clone(), rpc_addr).await?;
            println!(
                "Node started. Data dir: {}. RPC: {}.",
                data_dir.display(),
                rpc_server.bound
            );
            signal::ctrl_c().await?;
        }
        Commands::UploadBlob { rpc, file, mime } => {
            let metadata = tokio::fs::metadata(&file).await?;
            let total = metadata.len();
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})",
                )
                .unwrap(),
            );
            let f = tokio::fs::File::open(&file).await?;
            let pb_clone = pb.clone();
            let stream = ReaderStream::new(f).inspect_ok(move |chunk| {
                pb_clone.inc(chunk.len() as u64);
            });
            let body = Body::wrap_stream(stream);
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let mut req = client.post(format!("{endpoint}/blobs")).body(body);
            if let Some(m) = mime.clone() {
                req = req.header("x-mime", m);
            }
            let res = req.send().await?;
            pb.finish_and_clear();
            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::Deploy {
            rpc,
            file,
            entrypoint,
            blob_refs,
            salt,
        } => {
            let data = std::fs::read(&file)?;
            let body = serde_json::json!({
                "wasm_base64": general_purpose::STANDARD.encode(&data),
                "entrypoint": entrypoint,
                "blob_refs": blob_refs,
                "salt_base64": salt.or_else(|| {
                    let rand = uuid::Uuid::new_v4();
                    Some(general_purpose::STANDARD.encode(rand.as_bytes()))
                }),
            });
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .post(format!("{endpoint}/programs"))
                .json(&body)
                .send()
                .await?;
            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::SubmitJob {
            rpc,
            program_id,
            input,
            request_id,
            max_retries,
        } => {
            let input_base64 = if let Some(path) = input {
                let data = std::fs::read(path)?;
                Some(general_purpose::STANDARD.encode(&data))
            } else {
                None
            };

            let req_id = request_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let body = serde_json::json!({
                "request_id": req_id,
                "program_id": program_id,
                "input_base64": input_base64,
                "max_retries": max_retries,
            });

            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .post(format!("{endpoint}/jobs"))
                .json(&body)
                .send()
                .await?;

            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::JobStatus { rpc, job_id } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/jobs/{job_id}"))
                .send()
                .await?;

            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::JobLogs { rpc, job_id } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/jobs/{job_id}/logs"))
                .send()
                .await?;

            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::JobOutput { rpc, job_id, out } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/jobs/{job_id}/output"))
                .send()
                .await?;

            if !res.status().is_success() {
                return Err(anyhow::anyhow!(res.text().await?));
            }

            let response: serde_json::Value = res.json().await?;
            let output_base64 = response
                .get("output_base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing output_base64 in response"))?;

            let data = general_purpose::STANDARD.decode(output_base64)?;
            tokio::fs::write(&out, &data).await?;
            println!("output saved to {}", out.display());
        }
        Commands::CancelJob { rpc, job_id } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .post(format!("{endpoint}/jobs/{job_id}/cancel"))
                .send()
                .await?;

            if res.status().is_success() {
                println!("job {job_id} cancelled");
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::ListJobs { rpc } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client.get(format!("{endpoint}/jobs")).send().await?;

            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::Execute {
            rpc,
            program_id,
            input,
            json,
            estimate,
        } => {
            let data = if let Some(path) = input {
                std::fs::read(path)?
            } else {
                Vec::new()
            };

            if estimate {
                let body = serde_json::json!({
                    "program_id": program_id,
                    "input_base64": general_purpose::STANDARD.encode(&data),
                });
                let client = reqwest::Client::new();
                let endpoint = normalize_rpc_endpoint(&rpc);
                let res = client
                    .post(format!("{endpoint}/estimate-fuel"))
                    .json(&body)
                    .send()
                    .await?;
                let status = res.status();
                let text = res.text().await?;
                if !status.is_success() {
                    return Err(anyhow::anyhow!(text));
                }

                if json {
                    println!("{}", text);
                } else {
                    let v: serde_json::Value = serde_json::from_str(&text)?;
                    let estimated_fuel = v
                        .get("estimated_fuel")
                        .and_then(|f| f.as_u64())
                        .ok_or_else(|| anyhow::anyhow!("missing estimated_fuel"))?;
                    let confidence = v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
                    let samples = v
                        .get("based_on_samples")
                        .and_then(|s| s.as_u64())
                        .unwrap_or(0);
                    let exec_time = v
                        .get("estimated_execution_time_ms")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0);

                    println!("Estimated Fuel: {}", estimated_fuel);
                    println!("Confidence: {:.1}%", confidence * 100.0);
                    println!("Based on {} historical sample(s)", samples);
                    println!("Estimated Execution Time: {} ms", exec_time);
                }
            } else {
                let body = serde_json::json!({
                    "program_id": program_id,
                    "input_base64": general_purpose::STANDARD.encode(&data),
                });
                let client = reqwest::Client::new();
                let endpoint = normalize_rpc_endpoint(&rpc);
                let res = client
                    .post(format!("{endpoint}/execute"))
                    .json(&body)
                    .send()
                    .await?;
                let status = res.status();
                let text = res.text().await?;
                if !status.is_success() {
                    return Err(anyhow::anyhow!(text));
                }
                if json {
                    let v: serde_json::Value = serde_json::from_str(&text)?;
                    let ret_b64 = v
                        .get("return_base64")
                        .and_then(|s| s.as_str())
                        .ok_or_else(|| anyhow::anyhow!("missing return_base64 in response"))?;
                    let decoded = general_purpose::STANDARD.decode(ret_b64)?;
                    let decoded_str = String::from_utf8_lossy(&decoded).into_owned();
                    let fuel = v.get("fuel").cloned().unwrap_or(serde_json::json!(null));
                    let out = serde_json::json!({
                        "return": decoded_str,
                        "fuel": fuel,
                    });
                    println!("{}", serde_json::to_string(&out)?);
                } else {
                    let v: serde_json::Value = serde_json::from_str(&text)?;
                    let ret = v
                        .get("return_base64")
                        .and_then(|s| s.as_str())
                        .ok_or_else(|| anyhow::anyhow!("missing return_base64 in response"))?;
                    let decoded = general_purpose::STANDARD.decode(ret)?;
                    println!("{}", String::from_utf8_lossy(&decoded));
                }
            }
        }
        Commands::GetBlob { rpc, id, out } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client.get(format!("{endpoint}/blobs/{id}")).send().await?;
            if !res.status().is_success() {
                return Err(anyhow::anyhow!(res.text().await?));
            }
            let total = res.content_length().unwrap_or(0);
            let pb = if total > 0 {
                let pb = ProgressBar::new(total);
                pb.set_style(
                    ProgressStyle::with_template(
                        "[{elapsed_precise}] {bar:40.green/blue} {bytes}/{total_bytes} ({eta})",
                    )
                    .unwrap(),
                );
                Some(pb)
            } else {
                None
            };
            let mut file = tokio::fs::File::create(&out).await?;
            let mut stream = res.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                if let Some(pb) = pb.as_ref() {
                    pb.inc(chunk.len() as u64);
                }
            }
            file.flush().await?;
            if let Some(pb) = pb {
                pb.finish_and_clear();
            }
            println!("blob saved to {}", out.display());
        }
        Commands::ProgramInfo { rpc, id } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/programs/{id}"))
                .send()
                .await?;
            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
    }
    Ok(())
}
