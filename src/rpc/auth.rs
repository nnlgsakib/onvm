use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::config::RpcAuthConfig;
use crate::rpc::auth_store::AuthStore;
use anyhow::{anyhow, Context, Result};
use argon2::Argon2;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::Aead, aead::Payload, ChaCha20Poly1305, Key, KeyInit, Nonce, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand_core::OsRng;
use rand_core::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use tracing::warn;
use zeroize::Zeroize;

use std::fs;
use std::path::{Path, PathBuf};

const HDR_PROJECT_ID: &str = "x-project-id";
const HDR_TIMESTAMP: &str = "x-timestamp";
const HDR_NONCE: &str = "x-nonce";
const HDR_SIGNATURE: &str = "x-signature";
const HDR_RESPONSE_NONCE: &str = "x-response-nonce";
const HDR_RESPONSE_AEAD: &str = "x-response-aead";

const RPC_SECRET_HEADER: &[u8; 8] = b"ONVMRPC1";
const RPC_SALT_LEN: usize = 16;
const RPC_NONCE_LEN: usize = 12;

/// Signing + encryption material derived per project.
#[derive(Clone, Zeroize)]
#[zeroize(drop)]
struct DerivedKeys {
    project_id: String,
    signing_key: [u8; 32],
    response_key: [u8; 32],
}

#[derive(Clone)]
pub struct AuthConfig {
    pub project_id: String,
    pub skew: Duration,
    pub nonce_ttl: Duration,
    pub nonce_capacity: usize,
    pub allowed_methods: Vec<String>,
}

#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
    keys: HashMap<String, DerivedKeys>,
    replay_cache: Arc<Mutex<NonceCache>>,
    rate_limiter: Option<Arc<Mutex<TokenBucket>>>,
}

impl AuthState {
    pub fn from_config(
        cfg: &RpcAuthConfig,
        data_dir: &Path,
        identity_passphrase: Option<&str>,
    ) -> Result<Option<Self>> {
        if !cfg.enable {
            return Ok(None);
        }
        let passphrase = identity_passphrase
            .ok_or_else(|| anyhow!("rpc auth enabled but identity passphrase not provided"))?;
        let (keys, rate_limit_per_minute) = load_project_keys(cfg, data_dir, passphrase)?;
        let allowed_methods: Vec<String> = cfg
            .allowed_methods
            .iter()
            .map(|m| m.to_ascii_uppercase())
            .collect();
        let config = AuthConfig {
            project_id: cfg.project_id.clone(),
            skew: Duration::from_millis(cfg.skew_ms),
            nonce_ttl: Duration::from_millis(cfg.nonce_ttl_ms),
            nonce_capacity: cfg.nonce_capacity,
            allowed_methods,
        };
        Ok(Some(Self::new(config, keys, rate_limit_per_minute)?))
    }

    fn new(config: AuthConfig, keys: Vec<DerivedKeys>, rate_limit_per_minute: u64) -> Result<Self> {
        let limiter = if rate_limit_per_minute > 0 {
            Some(Arc::new(Mutex::new(TokenBucket::new(
                rate_limit_per_minute as f64,
            ))))
        } else {
            None
        };
        let mut map = HashMap::new();
        for k in keys {
            map.insert(k.project_id.clone(), k);
        }
        Ok(Self {
            replay_cache: Arc::new(Mutex::new(NonceCache::new(
                config.nonce_capacity,
                config.nonce_ttl,
            ))),
            config,
            keys: map,
            rate_limiter: limiter,
        })
    }
}

#[allow(dead_code)]
pub fn generate_project_secret() -> Result<(String, Vec<u8>, String)> {
    let mut project_id_bytes = [0u8; 16];
    let mut secret = vec![0u8; 32];
    OsRng.fill_bytes(&mut project_id_bytes);
    OsRng.fill_bytes(&mut secret);
    let project_id = hex::encode(project_id_bytes);
    let secret_hex = hex::encode(&secret);
    Ok((project_id, secret, secret_hex))
}

#[allow(dead_code)]
pub fn encrypt_project_secret(master_password: &str, secret: &[u8]) -> Result<String> {
    let mut salt_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut salt_bytes);
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(master_password.as_bytes(), &salt_bytes, &mut key)
        .map_err(|e| anyhow!("argon2: {e}"))?;
    let cipher = XChaCha20Poly1305::new(&key.into());
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce_bytes), secret)
        .map_err(|e| anyhow!("encrypt failed: {e}"))?;
    Ok(format!(
        "{}:{}:{}",
        hex::encode(salt_bytes),
        hex::encode(nonce_bytes),
        hex::encode(ciphertext)
    ))
}

pub async fn verify_signed_request(
    State(state): State<AuthState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.allowed_methods.is_empty() {
        let method = req.method().as_str().to_ascii_uppercase();
        if !state.config.allowed_methods.iter().any(|m| m == &method) {
            warn!("rpc auth: method {} not allowed", method);
            return Err(StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    let headers = req.headers();
    let project_id = header(headers, HDR_PROJECT_ID)?;
    let Some(project_keys) = state.keys.get(&project_id) else {
        warn!("rpc auth: project_id mismatch");
        return Err(StatusCode::UNAUTHORIZED);
    };

    let ts_ms: i64 = header(headers, HDR_TIMESTAMP)?
        .parse()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let nonce = header(headers, HDR_NONCE)?;
    let signature_hex = header(headers, HDR_SIGNATURE)?;
    let signature = hex::decode(&signature_hex).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let now_ms = now_millis();
    let delta = (now_ms as i128 - ts_ms as i128).abs();
    if delta as u128 > state.config.skew.as_millis() {
        warn!("rpc auth: timestamp skew too large");
        return Err(StatusCode::UNAUTHORIZED);
    }

    {
        let mut cache = state.replay_cache.lock().await;
        cache
            .check_and_insert(&nonce, Instant::now())
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
    }

    let (parts, body) = req.into_parts();
    // Honor the upstream body limit; we trust the DefaultBodyLimit layer to enforce size.
    let body_bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let canonical = format!(
        "{} {}\n{}\n{}\n{}",
        parts.method,
        path,
        ts_ms,
        nonce,
        String::from_utf8_lossy(&body_bytes)
    );

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&project_keys.signing_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(canonical.as_bytes());
    let expected = mac.finalize().into_bytes();
    if expected.as_slice().ct_eq(signature.as_slice()).unwrap_u8() != 1 {
        // warn!(
        //     "rpc auth: signature mismatch for project {} (path: {})",
        //     project_id, path
        // );
        return Err(StatusCode::UNAUTHORIZED);
    }
    let response_key = project_keys.response_key;

    if let Some(limiter) = &state.rate_limiter {
        let mut limiter = limiter.lock().await;
        if !limiter.try_consume(1.0) {
            warn!("rpc auth: rate limit exceeded for project {}", project_id);
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    let req = Request::from_parts(parts, Body::from(body_bytes.clone()));
    let resp = next.run(req).await;

    if !resp.status().is_success() {
        // warn!(
        //     "rpc auth: downstream handler returned error {}, skipping encryption",
        //     resp.status()
        // );
        return Ok(resp);
    }

    let (mut resp_parts, resp_body) = resp.into_parts();
    let plaintext = to_bytes(resp_body, usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response_nonce = derive_response_nonce(&response_key, &nonce, ts_ms)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(&response_key));
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&response_nonce),
            Payload {
                msg: &plaintext,
                aad: canonical.as_bytes(),
            },
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    resp_parts.headers.insert(
        HDR_RESPONSE_NONCE,
        header_value(hex::encode(response_nonce))?,
    );
    resp_parts
        .headers
        .insert(HDR_RESPONSE_AEAD, header_value("XChaCha20Poly1305")?);
    let encoded = general_purpose::STANDARD.encode(ciphertext);
    let mut response_key = response_key;
    response_key.zeroize();
    Ok(Response::from_parts(resp_parts, Body::from(encoded)))
}

fn load_project_keys(
    cfg: &RpcAuthConfig,
    data_dir: &Path,
    passphrase: &str,
) -> Result<(Vec<DerivedKeys>, u64)> {
    let store = open_store_from_config(cfg, data_dir, passphrase)?;
    store.ensure_project(&cfg.project_id)?;
    let secrets = store.load_all()?;

    let mut derived = Vec::new();
    for (project_id, mut secret) in secrets {
        let keys = derive_keys(&secret, &project_id)?;
        secret.zeroize();
        derived.push(keys);
    }
    Ok((derived, cfg.rate_limit_per_minute))
}

pub fn open_store_from_config(
    cfg: &RpcAuthConfig,
    data_dir: &Path,
    passphrase: &str,
) -> Result<AuthStore> {
    let Some(master_path) = &cfg.secret_path else {
        return Err(anyhow!(
            "rpc auth enabled but no secret_path provided in config"
        ));
    };
    let master_path = resolve_secret_path(data_dir, master_path);
    let master = read_and_decrypt_secret(&master_path, passphrase)?;
    if master.len() != 32 {
        return Err(anyhow!("rpc master secret must be 32 bytes"));
    }
    let mut master_arr = [0u8; 32];
    master_arr.copy_from_slice(&master);
    let store_path = data_dir.join("auth_store");
    AuthStore::open(store_path, master_arr)
}

pub fn encrypt_and_write_secret(path: &Path, passphrase: &str, secret: &[u8]) -> Result<()> {
    let mut salt = [0u8; RPC_SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce = [0u8; RPC_NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let key = derive_rpc_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), secret)
        .map_err(|e| anyhow!("encrypt rpc auth secret: {e}"))?;
    let mut out = Vec::with_capacity(
        RPC_SECRET_HEADER.len() + RPC_SALT_LEN + RPC_NONCE_LEN + ciphertext.len(),
    );
    out.extend_from_slice(RPC_SECRET_HEADER);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    fs::write(path, &out)
        .with_context(|| format!("writing rpc auth secret to {}", path.display()))?;
    Ok(())
}

fn read_and_decrypt_secret(path: &Path, passphrase: &str) -> Result<Vec<u8>> {
    let data = fs::read(path)
        .with_context(|| format!("reading rpc auth secret from {}", path.display()))?;
    if data.len() < RPC_SECRET_HEADER.len() + RPC_SALT_LEN + RPC_NONCE_LEN {
        return Err(anyhow!("rpc auth secret blob too short"));
    }
    if &data[..RPC_SECRET_HEADER.len()] != RPC_SECRET_HEADER {
        return Err(anyhow!("rpc auth secret missing header"));
    }
    let mut salt = [0u8; RPC_SALT_LEN];
    salt.copy_from_slice(&data[RPC_SECRET_HEADER.len()..RPC_SECRET_HEADER.len() + RPC_SALT_LEN]);
    let mut nonce = [0u8; RPC_NONCE_LEN];
    nonce.copy_from_slice(
        &data[RPC_SECRET_HEADER.len() + RPC_SALT_LEN
            ..RPC_SECRET_HEADER.len() + RPC_SALT_LEN + RPC_NONCE_LEN],
    );
    let ciphertext = &data[RPC_SECRET_HEADER.len() + RPC_SALT_LEN + RPC_NONCE_LEN..];
    let key = derive_rpc_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key);
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext)
        .map_err(|e| anyhow!("rpc auth secret decrypt failed: {e}"))?;
    Ok(plaintext)
}

fn derive_rpc_key(passphrase: &str, salt: &[u8; RPC_SALT_LEN]) -> Result<Key> {
    let mut key_bytes = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key_bytes)
        .map_err(|e| anyhow!("argon2: {e}"))?;
    Ok(*Key::from_slice(&key_bytes))
}

fn resolve_secret_path(data_dir: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        data_dir.join(p)
    }
}

fn derive_keys(secret: &[u8; 32], project_id: &str) -> Result<DerivedKeys> {
    let hk = Hkdf::<Sha256>::new(None, secret);
    let mut okm = [0u8; 64];
    hk.expand(b"onvm-rpc-auth", &mut okm)
        .map_err(|_| anyhow!("failed to derive keys"))?;
    let mut signing_key = [0u8; 32];
    let mut response_key = [0u8; 32];
    signing_key.copy_from_slice(&okm[0..32]);
    response_key.copy_from_slice(&okm[32..64]);
    Ok(DerivedKeys {
        project_id: project_id.to_string(),
        signing_key,
        response_key,
    })
}

fn derive_response_nonce(response_key: &[u8; 32], nonce: &str, ts_ms: i64) -> Result<[u8; 24]> {
    let hk = Hkdf::<Sha256>::new(Some(response_key), nonce.as_bytes());
    let mut out = [0u8; 24];
    hk.expand(format!("resp-{ts_ms}").as_bytes(), &mut out)
        .map_err(|_| anyhow!("failed to derive response nonce"))?;
    Ok(out)
}

fn header(headers: &HeaderMap, name: &str) -> Result<String, StatusCode> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)
}

fn header_value(value: impl AsRef<str>) -> Result<axum::http::HeaderValue, StatusCode> {
    axum::http::HeaderValue::from_str(value.as_ref()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn now_millis() -> i64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as i64
}

struct NonceCache {
    entries: HashMap<String, Instant>,
    order: VecDeque<String>,
    capacity: usize,
    ttl: Duration,
}

impl NonceCache {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity,
            ttl,
        }
    }

    fn check_and_insert(&mut self, nonce: &str, now: Instant) -> Result<(), ()> {
        self.evict(now);
        if self.entries.contains_key(nonce) {
            return Err(());
        }
        if self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(nonce.to_string(), now);
        self.order.push_back(nonce.to_string());
        Ok(())
    }

    fn evict(&mut self, now: Instant) {
        while let Some(front) = self.order.front() {
            let stale = self
                .entries
                .get(front)
                .map(|ts| now.duration_since(*ts) > self.ttl)
                .unwrap_or(true);
            if stale {
                if let Some(expired) = self.order.pop_front() {
                    self.entries.remove(&expired);
                }
            } else {
                break;
            }
        }
    }
}

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    last_refill: Instant,
    refill_per_sec: f64,
}

impl TokenBucket {
    fn new(rate_per_minute: f64) -> Self {
        let rate_per_sec = rate_per_minute / 60.0;
        Self {
            tokens: rate_per_sec,
            capacity: rate_per_sec * 2.0,
            last_refill: Instant::now(),
            refill_per_sec: rate_per_sec,
        }
    }

    fn try_consume(&mut self, amount: f64) -> bool {
        let now = Instant::now();
        let elapsed = now
            .checked_duration_since(self.last_refill)
            .unwrap_or_default()
            .as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }
}