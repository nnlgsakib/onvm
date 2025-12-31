use crate::node::{Node, NodeConfig};
use crate::rpc::start_rpc;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use clap::{Parser, Subcommand};
use futures::{StreamExt, TryStreamExt};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use indicatif::{ProgressBar, ProgressStyle};
use libp2p::Multiaddr;
use reqwest::Body;
use rpassword::prompt_password;
use sha2::Sha256;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio_util::io::ReaderStream;
use tracing_subscriber::EnvFilter;

fn derive_keys(project_secret: &str) -> Result<([u8; 32], [u8; 32])> {
    let secret_bytes = hex::decode(project_secret).context("project_secret must be hex-encoded")?;
    if secret_bytes.len() != 32 {
        anyhow::bail!("project_secret must be 32 bytes (64 hex chars)");
    }
    let mut secret_arr = [0u8; 32];
    secret_arr.copy_from_slice(&secret_bytes);

    let hk = Hkdf::<Sha256>::new(None, &secret_arr);
    let mut okm = [0u8; 64];
    hk.expand(b"onvm-rpc-auth", &mut okm)
        .map_err(|_| anyhow::anyhow!("failed to derive keys"))?;

    let mut signing_key = [0u8; 32];
    let mut response_key = [0u8; 32];
    signing_key.copy_from_slice(&okm[0..32]);
    response_key.copy_from_slice(&okm[32..64]);
    Ok((signing_key, response_key))
}

fn derive_response_nonce(response_key: &[u8; 32], nonce: &str, ts_ms: i64) -> Result<[u8; 24]> {
    let hk = Hkdf::<Sha256>::new(Some(response_key), nonce.as_bytes());
    let mut out = [0u8; 24];
    hk.expand(format!("resp-{ts_ms}").as_bytes(), &mut out)
        .map_err(|_| anyhow::anyhow!("failed to derive response nonce"))?;
    Ok(out)
}

struct AuthContext {
    headers: reqwest::header::HeaderMap,
    timestamp: i64,
    nonce: String,
    canonical: String,
    response_key: [u8; 32],
}

fn build_auth_headers(
    method: &str,
    path: &str,
    body: &str,
    project_id: &str,
    project_secret: &str,
) -> Result<AuthContext> {
    let (signing_key, response_key) = derive_keys(project_secret)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let nonce = uuid::Uuid::new_v4().to_string();

    let canonical = format!("{} {}\n{}\n{}\n{}", method, path, timestamp, nonce, body);

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&signing_key)
        .map_err(|e| anyhow::anyhow!("hmac error: {}", e))?;
    mac.update(canonical.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-project-id", project_id.parse()?);
    headers.insert("x-timestamp", timestamp.to_string().parse()?);
    headers.insert("x-nonce", nonce.clone().parse()?);
    headers.insert("x-signature", signature.parse()?);

    Ok(AuthContext {
        headers,
        timestamp,
        nonce,
        canonical,
        response_key,
    })
}

fn decrypt_response(
    ciphertext_b64: &str,
    response_key: &[u8; 32],
    nonce: &str,
    timestamp: i64,
    canonical: &str,
    response_nonce_hex: Option<&str>,
) -> Result<String> {
    let ciphertext = general_purpose::STANDARD
        .decode(ciphertext_b64)
        .context("failed to decode base64 response")?;

    let response_nonce = if let Some(hex) = response_nonce_hex {
        let bytes = hex::decode(hex).context("failed to decode response nonce hex")?;
        if bytes.len() != 24 {
            anyhow::bail!("invalid response nonce length");
        }
        let mut arr = [0u8; 24];
        arr.copy_from_slice(&bytes);
        arr
    } else {
        derive_response_nonce(response_key, nonce, timestamp)?
    };

    let cipher = XChaCha20Poly1305::new(response_key.into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&response_nonce),
            Payload {
                msg: &ciphertext,
                aad: canonical.as_bytes(),
            },
        )
        .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))?;

    Ok(String::from_utf8(plaintext)?)
}

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
    /// Generate a new RPC project and secret backed by the auth store
    GenerateProject {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "rpc")]
        rpc: String,
        #[arg(long, help = "Encrypt/decrypt secrets with this passphrase")]
        identity_passphrase: String,
        #[arg(long, help = "Optional project id to reuse; random if omitted")]
        project_id: Option<String>,
        #[arg(
            long,
            help = "Optional project secret (hex) to import; random if omitted"
        )]
        project_secret: Option<String>,
    },
    /// Verify project credentials against a node's RPC auth
    AuthCheck {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "rpc")]
        rpc: String,
        #[arg(long)]
        project_id: String,
        #[arg(long)]
        project_secret: String,
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
        #[arg(long, help = "Enable blob gateway CDN on separate port")]
        enable_gateway: bool,
        #[arg(
            long,
            default_value = "127.0.0.1:8081",
            help = "Blob gateway listen address"
        )]
        gateway_addr: String,
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
    /// List known program manifests
    ProgramCatalog {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
    },
    /// Show receipts for a program
    ProgramReceipts {
        #[arg(long, default_value = "127.0.0.1:8080", alias = "api")]
        rpc: String,
        #[arg(long)]
        id: String,
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
        #[arg(long, help = "Project ID for authentication")]
        project_id: Option<String>,
        #[arg(long, help = "Project secret for authentication")]
        project_secret: Option<String>,
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
        #[arg(long, help = "Project ID for authentication")]
        project_id: Option<String>,
        #[arg(long, help = "Project secret for authentication")]
        project_secret: Option<String>,
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
        #[arg(long, help = "Project ID for authentication")]
        project_id: Option<String>,
        #[arg(long, help = "Project secret for authentication")]
        project_secret: Option<String>,
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
            let mut identity_passphrase = identity_passphrase;
            let mut identity_keys: Option<crate::crypto::keys::NodeKeys> = None;

            if identity_path.exists() {
                println!(
                    "Identity already exists at {}. Skipping generation.",
                    identity_path.display()
                );
            } else {
                identity_passphrase = if identity_passphrase.is_none() && !allow_plaintext_identity
                {
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
                identity_keys = Some(keys);
            }

            let rpc_secret_path = data_dir.join("rpc_auth.secret");
            let rpc_secret_rel = rpc_secret_path
                .strip_prefix(&data_dir)
                .unwrap_or(&rpc_secret_path)
                .to_string_lossy()
                .to_string();
            if !rpc_secret_path.exists() {
                let rpc_secret_passphrase = identity_passphrase.clone().ok_or_else(|| {
                    anyhow::anyhow!(
                        "RPC auth secrets require --identity-passphrase; rerun init with a passphrase"
                    )
                })?;
                let (_, secret_bytes, _) = crate::rpc::auth::generate_project_secret()?;
                crate::rpc::auth::encrypt_and_write_secret(
                    &rpc_secret_path,
                    &rpc_secret_passphrase,
                    &secret_bytes,
                )?;
                println!("Wrote RPC auth secret to {}", rpc_secret_path.display());
            }

            let bls_identity_path = data_dir.join("bls_identity");
            if !bls_identity_path.exists() {
                if identity_keys.is_none() {
                    if identity_passphrase.is_none() && !allow_plaintext_identity {
                        identity_passphrase = Some(prompt_for_passphrase_once()?);
                    }

                    let keys = crate::crypto::keys::NodeKeys::load_or_generate(
                        &identity_path,
                        identity_passphrase.as_deref(),
                        allow_plaintext_identity,
                    )
                    .await?;
                    identity_keys = Some(keys);
                }

                let keys = identity_keys
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing identity keys for BLS init"))?;
                let (_sk, pk) =
                    crate::crypto::bls::generate_and_store_identity(&data_dir, keys).await?;
                println!(
                    "Generated BLS consensus identity at {} (pubkey {})",
                    bls_identity_path.display(),
                    hex::encode(pk.0)
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
                let mut cfg = crate::config::OnvmConfig::default();
                cfg.rpc_auth.enable = true;
                cfg.rpc_auth.secret_path = Some(rpc_secret_rel.clone());
                let content = cfg.to_toml();
                let text = toml::to_string_pretty(&content)?;
                std::fs::write(&cfg_path, text)?;
                println!("Wrote default config to {}", cfg_path.display());
            } else if cfg_path.exists() || overrides_present {
                let mut cfg = if cfg_path.exists() {
                    crate::config::OnvmConfig::from_file(&cfg_path).unwrap_or_default()
                } else {
                    crate::config::OnvmConfig::default()
                };

                // Ensure RPC auth is enabled with the generated secret path.
                if cfg.rpc_auth.secret_path.is_none() {
                    cfg.rpc_auth.secret_path = Some(rpc_secret_rel.clone());
                }
                cfg.rpc_auth.enable = true;

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
            enable_gateway,
            gateway_addr,
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
                onvm_cfg.rpc_auth.enable = false;
                println!("Dev mode: RPC auth disabled regardless of config.toml");
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
                        max_total_connections: onvm_cfg.network.max_total_connections,
                        max_connections_per_peer: onvm_cfg.network.max_connections_per_peer,
                        max_inbound_streams: onvm_cfg.network.max_inbound_streams,
                        max_gossip_bytes: onvm_cfg.network.max_gossip_bytes,
                        pex: onvm_cfg.network.pex.clone(),
                        keep_alive: onvm_cfg.network.keep_alive.clone(),
                        memory_throttle: onvm_cfg.network.memory_throttle.clone(),
                    },
                    bootnodes: parsed_bootnodes,
                })
                .await?,
            );
            let rpc_server = start_rpc(
                node.clone(),
                rpc_addr,
                onvm_cfg.rpc_auth.clone(),
                data_dir.clone(),
                identity_passphrase.clone(),
            )
            .await?;

            let gateway_handle = if enable_gateway {
                let gateway_config = crate::blob_gateway::GatewayConfig {
                    listen_addr: gateway_addr.clone(),
                };
                let node_clone = node.clone();
                Some(tokio::spawn(async move {
                    if let Err(e) =
                        crate::blob_gateway::start_gateway(node_clone, gateway_config).await
                    {
                        tracing::error!("Gateway error: {}", e);
                    }
                }))
            } else {
                None
            };

            println!(
                "Node started. Data dir: {}. RPC: {}.",
                data_dir.display(),
                rpc_server.bound
            );
            if enable_gateway {
                println!("Blob Gateway: http://{}", gateway_addr);
            }
            signal::ctrl_c().await?;

            if let Some(handle) = gateway_handle {
                handle.abort();
            }
        }
        Commands::GenerateProject {
            rpc,
            identity_passphrase,
            project_id,
            project_secret,
        } => {
            let endpoint = normalize_rpc_endpoint(&rpc);
            let body = serde_json::json!({
                "identity_passphrase": identity_passphrase,
                "project_id": project_id,
                "project_secret": project_secret,
            });
            let client = reqwest::Client::new();
            let res = client
                .post(format!("{endpoint}/rpc-auth/projects"))
                .json(&body)
                .send()
                .await
                .with_context(|| format!("failed to reach RPC at {endpoint}/rpc-auth/projects"))?;
            let status = res.status();
            let text = res.text().await?;
            if !status.is_success() {
                let msg = if text.is_empty() {
                    format!("RPC returned status {} with empty body", status)
                } else {
                    text
                };
                return Err(anyhow::anyhow!(msg));
            }
            let v: serde_json::Value = serde_json::from_str(&text)?;
            let pid = v
                .get("project_id")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            let secret = v
                .get("project_secret")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            println!("project_id={}", pid);
            println!("project_secret={}", secret);
        }
        Commands::AuthCheck {
            rpc,
            project_id,
            project_secret,
        } => {
            let endpoint = normalize_rpc_endpoint(&rpc);
            let client = reqwest::Client::new();
            let body_str = "";
            let auth_ctx =
                build_auth_headers("GET", "/health", body_str, &project_id, &project_secret)?;
            let mut req = client.get(format!("{endpoint}/health"));
            for (key, value) in auth_ctx.headers.iter() {
                req = req.header(key, value);
            }
            let res = req.send().await?;
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            if status.is_success() {
                println!("ok");
            } else {
                eprintln!("auth check failed: HTTP {}", status);
                if !text.is_empty() {
                    eprintln!("Server response: {}", text);
                }
                return Err(anyhow::anyhow!("auth check failed"));
            }
        }
        Commands::UploadBlob {
            rpc,
            file,
            mime,
            project_id,
            project_secret,
        } => {
            let metadata = tokio::fs::metadata(&file).await?;
            let total = metadata.len();
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})",
                )
                .unwrap(),
            );

            let blob_data = std::fs::read(&file)?;

            let detected_mime = if mime.is_none() {
                let detector = crate::blob_gateway::content_detector::ContentDetector::new();
                Some(detector.detect(&blob_data))
            } else {
                None
            };

            let final_mime = mime.or(detected_mime);

            let body_str = String::from_utf8_lossy(&blob_data).to_string();

            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let mut req = client.post(format!("{endpoint}/blobs"));

            if let Some(m) = final_mime.clone() {
                req = req.header("x-mime", m);
            }

            if let (Some(id), Some(secret)) = (&project_id, &project_secret) {
                let ctx = build_auth_headers("POST", "/blobs", &body_str, id, secret)?;
                for (key, value) in ctx.headers.iter() {
                    req = req.header(key, value);
                }
            }

            let f = tokio::fs::File::open(&file).await?;
            let pb_clone = pb.clone();
            let stream = ReaderStream::new(f).inspect_ok(move |chunk| {
                pb_clone.inc(chunk.len() as u64);
            });
            let body = Body::wrap_stream(stream);
            req = req.body(body);

            let res = req.send().await?;
            pb.finish_and_clear();
            let status = res.status();
            let text = res.text().await?;

            if status.is_success() {
                println!("{}", text);
            } else if status == reqwest::StatusCode::UNAUTHORIZED {
                eprintln!("Authentication failed (401 Unauthorized).");
                eprintln!("Check that:");
                eprintln!("- --project-id/--project-secret are correct");
                eprintln!("- the project is registered on this node (run `onvm generate-project --rpc ... --identity-passphrase ... --project-id ... --project-secret ...` to import)");
                return Err(anyhow::anyhow!(
                    "Unauthorized: Missing or invalid project credentials"
                ));
            } else {
                eprintln!("Blob upload failed with status: {}", status);
                if !text.is_empty() {
                    eprintln!("Server response: {}", text);
                }
                return Err(anyhow::anyhow!("Upload blob failed: HTTP {}", status));
            }
        }
        Commands::Deploy {
            rpc,
            file,
            entrypoint,
            blob_refs,
            salt,
            project_id,
            project_secret,
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
            let body_str = serde_json::to_string(&body)?;
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let mut req = client
                .post(format!("{endpoint}/programs"))
                .header("content-type", "application/json")
                .body(body_str.clone());

            let auth_ctx = if let (Some(id), Some(secret)) = (&project_id, &project_secret) {
                let ctx = build_auth_headers("POST", "/programs", &body_str, id, secret)?;
                for (key, value) in ctx.headers.iter() {
                    req = req.header(key, value);
                }
                Some(ctx)
            } else {
                None
            };

            let res = req.send().await?;
            let status = res.status();
            let response_nonce = res
                .headers()
                .get("x-response-nonce")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let text = res.text().await?;

            let final_text = if let Some(ctx) = auth_ctx {
                if !text.is_empty() && response_nonce.is_some() {
                    match decrypt_response(
                        &text,
                        &ctx.response_key,
                        &ctx.nonce,
                        ctx.timestamp,
                        &ctx.canonical,
                        response_nonce.as_deref(),
                    ) {
                        Ok(decrypted) => decrypted,
                        Err(e) => {
                            eprintln!("Warning: Failed to decrypt response: {}", e);
                            text
                        }
                    }
                } else {
                    text
                }
            } else {
                text
            };

            if status.is_success() {
                println!("{}", final_text);
            } else if status == reqwest::StatusCode::UNAUTHORIZED {
                eprintln!("Authentication failed (401 Unauthorized).");
                eprintln!("Check that:");
                eprintln!("- --project-id/--project-secret are correct");
                eprintln!("- the project is registered on this node (run `onvm generate-project --rpc ... --identity-passphrase ... --project-id ... --project-secret ...` to import)");
                return Err(anyhow::anyhow!(
                    "Unauthorized: Missing or invalid project credentials"
                ));
            } else {
                eprintln!("Deploy failed with status: {}", status);
                if !final_text.is_empty() {
                    eprintln!("Server response: {}", final_text);
                }
                return Err(anyhow::anyhow!("Deploy failed: HTTP {}", status));
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
            project_id,
            project_secret,
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
                let body_str = serde_json::to_string(&body)?;
                let client = reqwest::Client::new();
                let endpoint = normalize_rpc_endpoint(&rpc);
                let mut req = client
                    .post(format!("{endpoint}/estimate-fuel"))
                    .header("content-type", "application/json")
                    .body(body_str.clone());

                let auth_ctx = if let (Some(id), Some(secret)) = (&project_id, &project_secret) {
                    let ctx = build_auth_headers("POST", "/estimate-fuel", &body_str, id, secret)?;
                    for (key, value) in ctx.headers.iter() {
                        req = req.header(key, value);
                    }
                    Some(ctx)
                } else {
                    None
                };

                let res = req.send().await?;
                let status = res.status();
                let response_nonce = res
                    .headers()
                    .get("x-response-nonce")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let text = res.text().await?;

                let final_text = if let Some(ctx) = auth_ctx {
                    if !text.is_empty() && response_nonce.is_some() {
                        match decrypt_response(
                            &text,
                            &ctx.response_key,
                            &ctx.nonce,
                            ctx.timestamp,
                            &ctx.canonical,
                            response_nonce.as_deref(),
                        ) {
                            Ok(decrypted) => decrypted,
                            Err(e) => {
                                eprintln!("Warning: Failed to decrypt response: {}", e);
                                text
                            }
                        }
                    } else {
                        text
                    }
                } else {
                    text
                };

                if !status.is_success() {
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        eprintln!("Authentication failed: The node requires project credentials.");
                        eprintln!("Please provide --project-id and --project-secret flags.");
                        return Err(anyhow::anyhow!(
                            "Unauthorized: Missing or invalid project credentials"
                        ));
                    } else {
                        eprintln!("Fuel estimation failed with status: {}", status);
                        if !final_text.is_empty() {
                            eprintln!("Server response: {}", final_text);
                        }
                        return Err(anyhow::anyhow!("Estimate fuel failed: HTTP {}", status));
                    }
                }

                if json {
                    println!("{}", final_text);
                } else {
                    let v: serde_json::Value = serde_json::from_str(&final_text)?;
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
                let body_str = serde_json::to_string(&body)?;
                let client = reqwest::Client::new();
                let endpoint = normalize_rpc_endpoint(&rpc);
                let mut req = client
                    .post(format!("{endpoint}/execute"))
                    .header("content-type", "application/json")
                    .body(body_str.clone());

                let auth_ctx = if let (Some(id), Some(secret)) = (&project_id, &project_secret) {
                    let ctx = build_auth_headers("POST", "/execute", &body_str, id, secret)?;
                    for (key, value) in ctx.headers.iter() {
                        req = req.header(key, value);
                    }
                    Some(ctx)
                } else {
                    None
                };

                let res = req.send().await?;
                let status = res.status();
                let response_nonce = res
                    .headers()
                    .get("x-response-nonce")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let text = res.text().await?;

                let final_text = if let Some(ctx) = auth_ctx {
                    if !text.is_empty() && response_nonce.is_some() {
                        match decrypt_response(
                            &text,
                            &ctx.response_key,
                            &ctx.nonce,
                            ctx.timestamp,
                            &ctx.canonical,
                            response_nonce.as_deref(),
                        ) {
                            Ok(decrypted) => decrypted,
                            Err(e) => {
                                eprintln!("Warning: Failed to decrypt response: {}", e);
                                text
                            }
                        }
                    } else {
                        text
                    }
                } else {
                    text
                };

                if !status.is_success() {
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        eprintln!("Authentication failed: The node requires project credentials.");
                        eprintln!("Please provide --project-id and --project-secret flags.");
                        return Err(anyhow::anyhow!(
                            "Unauthorized: Missing or invalid project credentials"
                        ));
                    } else {
                        eprintln!("Execution failed with status: {}", status);
                        if !final_text.is_empty() {
                            eprintln!("Server response: {}", final_text);
                        }
                        return Err(anyhow::anyhow!("Execute failed: HTTP {}", status));
                    }
                }
                if json {
                    let v: serde_json::Value = serde_json::from_str(&final_text)?;
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
                    let v: serde_json::Value = serde_json::from_str(&final_text)?;
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
        Commands::ProgramCatalog { rpc } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/program-catalog"))
                .send()
                .await?;
            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
        }
        Commands::ProgramReceipts { rpc, id } => {
            let client = reqwest::Client::new();
            let endpoint = normalize_rpc_endpoint(&rpc);
            let res = client
                .get(format!("{endpoint}/programs/{id}/receipts"))
                .send()
                .await?;
            if res.status().is_success() {
                println!("{}", res.text().await?);
            } else {
                return Err(anyhow::anyhow!(res.text().await?));
            }
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
