use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{aead::Aead, KeyInit, XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use sled::Db;
use std::collections::HashMap;
use std::path::Path;
use zeroize::Zeroize;

const STORE_TREE: &str = "projects";
const STORE_HEADER: &[u8; 8] = b"ONVMAP01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

pub struct AuthStore {
    db: Db,
    master: [u8; 32],
}

impl AuthStore {
    pub fn open(path: impl AsRef<Path>, master: [u8; 32]) -> Result<Self> {
        let db = sled::open(path.as_ref())
            .with_context(|| format!("opening auth store at {}", path.as_ref().display()))?;
        Ok(Self { db, master })
    }

    pub fn ensure_project(&self, project_id: &str) -> Result<[u8; 32]> {
        let tree = self.db.open_tree(STORE_TREE)?;
        if let Some(existing) = tree.get(project_id.as_bytes())? {
            let secret = self.decrypt(existing.as_ref())?;
            return Ok(secret);
        }
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let enc = self.encrypt(&secret)?;
        tree.insert(project_id.as_bytes(), enc)?;
        tree.flush()?;
        Ok(secret)
    }

    pub fn load_all(&self) -> Result<HashMap<String, [u8; 32]>> {
        let tree = self.db.open_tree(STORE_TREE)?;
        let mut out = HashMap::new();
        for item in tree.iter() {
            let (k, v) = item?;
            let key =
                String::from_utf8(k.to_vec()).map_err(|_| anyhow!("invalid project id utf8"))?;
            let secret = self.decrypt(v.as_ref())?;
            out.insert(key, secret);
        }
        Ok(out)
    }

    fn encrypt(&self, secret: &[u8; 32]) -> Result<Vec<u8>> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let key = self.derive_key(&salt)?;
        let cipher = XChaCha20Poly1305::new(&key);
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), secret.as_slice())
            .map_err(|e| anyhow!("encrypt project secret: {e}"))?;
        let mut out =
            Vec::with_capacity(STORE_HEADER.len() + SALT_LEN + NONCE_LEN + ciphertext.len());
        out.extend_from_slice(STORE_HEADER);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, data: &[u8]) -> Result<[u8; 32]> {
        if data.len() < STORE_HEADER.len() + SALT_LEN + NONCE_LEN {
            return Err(anyhow!("rpc auth store entry too short"));
        }
        if &data[..STORE_HEADER.len()] != STORE_HEADER {
            return Err(anyhow!("rpc auth store header mismatch"));
        }
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&data[STORE_HEADER.len()..STORE_HEADER.len() + SALT_LEN]);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(
            &data[STORE_HEADER.len() + SALT_LEN..STORE_HEADER.len() + SALT_LEN + NONCE_LEN],
        );
        let ciphertext = &data[STORE_HEADER.len() + SALT_LEN + NONCE_LEN..];
        let key = self.derive_key(&salt)?;
        let cipher = XChaCha20Poly1305::new(&key);
        let plaintext = cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext)
            .map_err(|e| anyhow!("rpc auth store decrypt failed: {e}"))?;
        if plaintext.len() != 32 {
            return Err(anyhow!("invalid project secret length"));
        }
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&plaintext);
        Ok(secret)
    }

    fn derive_key(&self, salt: &[u8; SALT_LEN]) -> Result<chacha20poly1305::Key> {
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), &self.master);
        let mut okm = [0u8; 32];
        hk.expand(b"onvm-rpc-auth-store", &mut okm)
            .map_err(|_| anyhow!("hkdf expand failed"))?;
        Ok(*chacha20poly1305::Key::from_slice(&okm))
    }
}

impl Drop for AuthStore {
    fn drop(&mut self) {
        self.master.zeroize();
    }
}
