use crate::node::{Node, NodeConfig};
use crate::rpc::start_rpc;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use clap::{Parser, Subcommand};
use futures::{StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use libp2p::Multiaddr;
use reqwest::Body;
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
        Commands::Init { data_dir } => {
            tokio::fs::create_dir_all(&data_dir).await?;
            let keys =
                crate::crypto::keys::NodeKeys::load_or_generate(data_dir.join("identity")).await?;
            let cfg_path = data_dir.join("config.toml");
            if !cfg_path.exists() {
                crate::config::OnvmConfig::write_to(&cfg_path)?;
            }
            println!(
                "Initialized identity at {}. Node ID: {}",
                data_dir.join("identity").display(),
                keys.node_id
            );
        }
        Commands::RunNode {
            data_dir,
            listen,
            rpc,
            min_peers,
            blob_sync_mode,
        } => {
            let listen_addr: Multiaddr = listen
                .parse()
                .with_context(|| format!("invalid listen multiaddr {listen}"))?;
            let rpc_endpoint = normalize_rpc_endpoint(&rpc);
            let rpc_addr: SocketAddr = rpc_endpoint
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .parse()?;
            let identity = Arc::new(
                crate::crypto::keys::NodeKeys::load_or_generate(data_dir.join("identity")).await?,
            );
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
