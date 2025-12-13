use crate::types::NodeId;
use anyhow::{anyhow, Context, Result};
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer};
use rand::rngs::OsRng;
use std::path::{Path, PathBuf};
use tokio::fs;

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
    pub async fn load_or_generate(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            let data = fs::read(&path)
                .await
                .with_context(|| format!("reading identity from {}", path.display()))?;
            let secret_hex = String::from_utf8(data)?;
            let secret_bytes = hex::decode(secret_hex.trim())?;
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
        fs::write(&path, hex::encode(keypair.secret.to_bytes()))
            .await
            .with_context(|| format!("writing identity to {}", path.display()))?;
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
}
