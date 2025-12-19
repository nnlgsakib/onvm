use crate::types::NodeId;
use anyhow::{anyhow, Context, Result};
use argon2::Argon2;
use base64::engine::general_purpose;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer};
use rand::rngs::OsRng;
use rand::RngCore;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing;
use zeroize::Zeroize;

const ENC_HEADER: &[u8; 8] = b"ONVMID01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub struct NodeKeys {
    pub keypair: Keypair,
    pub node_id: NodeId,
    #[allow(dead_code)]
    path: PathBuf,
}

impl Clone for NodeKeys {
    fn clone(&self) -> Self {
        let secret_bytes = self.keypair.secret.to_bytes();
        let secret = SecretKey::from_bytes(&secret_bytes).expect("valid secret");
        let public = PublicKey::from(&secret);
        let keypair = Keypair { secret, public };
        let node_id = NodeId::from_public_key(keypair.public.as_bytes());
        Self {
            keypair,
            node_id,
            path: self.path.clone(),
        }
    }
}

impl NodeKeys {
    pub async fn load_or_generate(
        path: impl AsRef<Path>,
        passphrase: Option<&str>,
        allow_plaintext: bool,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            let data = fs::read(&path)
                .await
                .with_context(|| format!("reading identity from {}", path.display()))?;

            if let Ok(decoded) = general_purpose::STANDARD.decode(&data) {
                if decoded.len() > ENC_HEADER.len() + SALT_LEN + NONCE_LEN
                    && &decoded[..ENC_HEADER.len()] == ENC_HEADER
                {
                    let Some(pass) = passphrase else {
                        return Err(anyhow!(
                            "identity is encrypted; provide --identity-passphrase"
                        ));
                    };
                    let secret = decrypt_secret(pass, &decoded)?;
                    let kp = secret_to_keypair(&secret)?;
                    return Ok(Self {
                        node_id: NodeId::from_public_key(kp.public.as_bytes()),
                        keypair: kp,
                        path,
                    });
                }
            }

            if !allow_plaintext {
                return Err(anyhow!(
                    "plaintext identity is disallowed; set --allow-plaintext-identity to override"
                ));
            }

            let secret_hex = String::from_utf8(data)?;
            let secret_bytes = hex::decode(secret_hex.trim())?;
            tracing::warn!("loading plaintext identity; consider setting a passphrase");
            let secret =
                SecretKey::from_bytes(&secret_bytes).map_err(|e| anyhow!("secret parse: {e}"))?;
            let public = PublicKey::from(&secret);
            let keypair = Keypair { secret, public };
            let node_id = NodeId::from_public_key(keypair.public.as_bytes());
            return Ok(Self {
                keypair,
                node_id,
                path,
            });
        }

        let mut rng = OsRng {};
        let keypair = Keypair::generate(&mut rng);
        let node_id = NodeId::from_public_key(keypair.public.as_bytes());
        if let Some(pass) = passphrase {
            let encrypted = encrypt_secret(pass, &keypair.secret.to_bytes())?;
            fs::write(&path, encrypted)
                .await
                .with_context(|| format!("writing encrypted identity to {}", path.display()))?;
        } else {
            fs::write(&path, hex::encode(keypair.secret.to_bytes()))
                .await
                .with_context(|| format!("writing identity to {}", path.display()))?;
        }
        Ok(Self {
            keypair,
            node_id,
            path,
        })
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.keypair.sign(data)
    }

    pub fn verify(&self, data: &[u8], signature: &Signature, public: &PublicKey) -> Result<()> {
        public.verify_strict(data, signature)?;
        Ok(())
    }

    pub fn from_keypair(keypair: Keypair, path: PathBuf) -> Self {
        let node_id = NodeId::from_public_key(keypair.public.as_bytes());
        Self {
            keypair,
            node_id,
            path,
        }
    }
}

fn secret_to_keypair(secret_bytes: &[u8]) -> Result<Keypair> {
    let secret = SecretKey::from_bytes(secret_bytes).map_err(|e| anyhow!("secret parse: {e}"))?;
    let public = PublicKey::from(&secret);
    Ok(Keypair { secret, public })
}

fn derive_key(passphrase: &str, salt: &[u8; SALT_LEN]) -> Result<Key> {
    let mut key_bytes = [0u8; 32];
    let argon = Argon2::default();
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key_bytes)
        .map_err(|e| anyhow!("kdf failed: {e}"))?;
    Ok(*Key::from_slice(&key_bytes))
}

fn encrypt_secret(passphrase: &str, secret_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, secret_bytes)
        .map_err(|e| anyhow!("encrypt identity: {e}"))?;
    let mut out = Vec::with_capacity(ENC_HEADER.len() + SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(ENC_HEADER);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    let mut key_bytes = key.to_vec();
    key_bytes.zeroize();
    Ok(general_purpose::STANDARD.encode(out).into_bytes())
}

fn decrypt_secret(passphrase: &str, encoded: &[u8]) -> Result<Vec<u8>> {
    let raw = encoded;
    if raw.len() < ENC_HEADER.len() + SALT_LEN + NONCE_LEN {
        return Err(anyhow!("identity blob too short"));
    }
    if &raw[..ENC_HEADER.len()] != ENC_HEADER {
        return Err(anyhow!("missing identity header"));
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&raw[ENC_HEADER.len()..ENC_HEADER.len() + SALT_LEN]);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(
        &raw[ENC_HEADER.len() + SALT_LEN..ENC_HEADER.len() + SALT_LEN + NONCE_LEN],
    );
    let ciphertext = &raw[ENC_HEADER.len() + SALT_LEN + NONCE_LEN..];
    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("identity decrypt failed: {e}"))?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use std::fs;
    use tokio::runtime::Runtime;
    use uuid::Uuid;

    fn tmp_path() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("onvm-id-{}", Uuid::new_v4()));
        p
    }

    #[test]
    fn encrypted_identity_roundtrip() -> Result<()> {
        let rt = Runtime::new()?;
        let path = tmp_path();
        let pass = "test-passphrase";

        let keys = rt.block_on(NodeKeys::load_or_generate(&path, Some(pass), false))?;
        let loaded = rt.block_on(NodeKeys::load_or_generate(&path, Some(pass), false))?;
        assert_eq!(keys.node_id, loaded.node_id);

        let data = fs::read(&path)?;
        let decoded = general_purpose::STANDARD.decode(&data)?;
        assert_eq!(&decoded[..ENC_HEADER.len()], ENC_HEADER);
        fs::remove_file(&path).ok();
        Ok(())
    }

    #[test]
    fn plaintext_disallowed_without_override() {
        let rt = Runtime::new().unwrap();
        let path = tmp_path();
        // write a random plaintext secret
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let _ = fs::write(&path, hex::encode(secret));

        let res = rt.block_on(NodeKeys::load_or_generate(&path, None, false));
        assert!(res.is_err());
        let _ = fs::remove_file(&path);
    }
}
