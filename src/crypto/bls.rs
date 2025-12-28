use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use blst::min_sig::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature};
use blst::BLST_ERROR;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::de::{Error as DeError, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Sha256;
use std::path::{Path, PathBuf};

pub const DST_RECEIPT: &[u8] = b"ONVM:RECEIPT:V1";
pub const DST_COMMITTEE: &[u8] = b"ONVM:COMMITTEE:V1";

const BLS_KEY_HEADER: &[u8; 8] = b"ONVMBLS1";
const BLS_IDENTITY_FILE: &str = "bls_identity";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsPublicKey(pub [u8; 96]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsSignature(pub [u8; 48]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsSecretKey(pub [u8; 32]);

fn deserialize_fixed_bytes<'de, const N: usize, D>(
    deserializer: D,
    type_name: &'static str,
) -> Result<[u8; N], D::Error>
where
    D: Deserializer<'de>,
{
    struct FixedBytesVisitor<const N: usize> {
        type_name: &'static str,
    }

    impl<'de, const N: usize> Visitor<'de> for FixedBytesVisitor<N> {
        type Value = [u8; N];

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{N} bytes for {}", self.type_name)
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            if v.len() != N {
                return Err(E::invalid_length(v.len(), &self));
            }
            let mut out = [0u8; N];
            out.copy_from_slice(v);
            Ok(out)
        }

        fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            self.visit_bytes(v)
        }

        fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            self.visit_bytes(&v)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut out = [0u8; N];
            for i in 0..N {
                match seq.next_element::<u8>()? {
                    Some(b) => out[i] = b,
                    None => return Err(A::Error::invalid_length(i, &self)),
                }
            }

            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::invalid_length(N + 1, &self));
            }

            Ok(out)
        }
    }

    deserializer.deserialize_bytes(FixedBytesVisitor::<N> { type_name })
}

impl Serialize for BlsPublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlsPublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(BlsPublicKey(deserialize_fixed_bytes::<96, _>(
            deserializer,
            "bls public key",
        )?))
    }
}

impl Serialize for BlsSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlsSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(BlsSignature(deserialize_fixed_bytes::<48, _>(
            deserializer,
            "bls signature",
        )?))
    }
}

impl Serialize for BlsSecretKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlsSecretKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(BlsSecretKey(deserialize_fixed_bytes::<32, _>(
            deserializer,
            "bls secret key",
        )?))
    }
}

pub fn generate_keypair() -> Result<(BlsSecretKey, BlsPublicKey)> {
    let mut ikm = [0u8; 32];
    OsRng.fill_bytes(&mut ikm);
    let sk = SecretKey::key_gen(&ikm, &[]).map_err(|e| anyhow!("bls keygen: {e:?}"))?;
    let pk = sk.sk_to_pk();
    Ok((BlsSecretKey(sk.to_bytes()), BlsPublicKey(pk.to_bytes())))
}

pub fn public_from_secret(secret: &BlsSecretKey) -> Result<BlsPublicKey> {
    let sk = SecretKey::from_bytes(&secret.0).map_err(|e| anyhow!("bls sk parse: {e:?}"))?;
    let pk = sk.sk_to_pk();
    Ok(BlsPublicKey(pk.to_bytes()))
}

pub async fn load_identity(
    data_dir: impl AsRef<Path>,
    identity: &crate::crypto::keys::NodeKeys,
) -> Result<(BlsSecretKey, BlsPublicKey)> {
    let path = identity_path(data_dir.as_ref());
    let data = tokio::fs::read(&path)
        .await
        .with_context(|| format!("reading BLS identity from {}", path.display()))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .context("decoding BLS identity base64")?;

    if decoded.len() < BLS_KEY_HEADER.len() + 24 {
        anyhow::bail!("BLS identity blob too short");
    }
    if &decoded[..BLS_KEY_HEADER.len()] != BLS_KEY_HEADER {
        anyhow::bail!("invalid BLS identity header");
    }

    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&decoded[BLS_KEY_HEADER.len()..BLS_KEY_HEADER.len() + 24]);
    let ciphertext = &decoded[BLS_KEY_HEADER.len() + 24..];

    let key = derive_bls_storage_key(identity)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext,
                aad: &identity.node_id.0,
            },
        )
        .map_err(|_| anyhow!("decrypting BLS identity"))?;

    if plaintext.len() != 32 {
        anyhow::bail!("invalid decrypted BLS secret key length");
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&plaintext);
    let secret = BlsSecretKey(sk);
    let public = public_from_secret(&secret)?;
    Ok((secret, public))
}

pub async fn generate_and_store_identity(
    data_dir: impl AsRef<Path>,
    identity: &crate::crypto::keys::NodeKeys,
) -> Result<(BlsSecretKey, BlsPublicKey)> {
    let (secret, public) = generate_keypair()?;
    store_identity(data_dir, identity, &secret).await?;
    Ok((secret, public))
}

pub async fn ensure_identity(
    data_dir: impl AsRef<Path>,
    identity: &crate::crypto::keys::NodeKeys,
) -> Result<(BlsSecretKey, BlsPublicKey)> {
    let path = identity_path(data_dir.as_ref());
    if path.exists() {
        return load_identity(data_dir, identity).await;
    }
    generate_and_store_identity(data_dir, identity).await
}

async fn store_identity(
    data_dir: impl AsRef<Path>,
    identity: &crate::crypto::keys::NodeKeys,
    secret: &BlsSecretKey,
) -> Result<()> {
    let path = identity_path(data_dir.as_ref());
    let key = derive_bls_storage_key(identity)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &secret.0,
                aad: &identity.node_id.0,
            },
        )
        .map_err(|_| anyhow!("encrypting BLS identity"))?;

    let mut out = Vec::with_capacity(BLS_KEY_HEADER.len() + nonce.len() + ciphertext.len());
    out.extend_from_slice(BLS_KEY_HEADER);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    let encoded = base64::engine::general_purpose::STANDARD.encode(out);
    tokio::fs::write(&path, encoded)
        .await
        .with_context(|| format!("writing BLS identity to {}", path.display()))?;
    Ok(())
}

fn identity_path(data_dir: &Path) -> PathBuf {
    data_dir.join(BLS_IDENTITY_FILE)
}

fn derive_bls_storage_key(identity: &crate::crypto::keys::NodeKeys) -> Result<[u8; 32]> {
    let secret_bytes = identity.keypair.secret.to_bytes();
    let hk = Hkdf::<Sha256>::new(None, &secret_bytes);
    let mut out = [0u8; 32];
    hk.expand(b"onvm-bls-identity", &mut out)
        .map_err(|_| anyhow!("failed to derive BLS storage key"))?;
    Ok(out)
}

pub fn sign(message: &[u8], secret: &BlsSecretKey, dst: &[u8]) -> Result<BlsSignature> {
    let sk = SecretKey::from_bytes(&secret.0).map_err(|e| anyhow!("bls sk parse: {e:?}"))?;
    let sig = sk.sign(message, dst, &[]);
    Ok(BlsSignature(sig.to_bytes()))
}

pub fn verify(
    message: &[u8],
    public: &BlsPublicKey,
    signature: &BlsSignature,
    dst: &[u8],
) -> Result<()> {
    let pk = PublicKey::from_bytes(&public.0).map_err(|e| anyhow!("bls pk parse: {e:?}"))?;
    let sig = Signature::from_bytes(&signature.0).map_err(|e| anyhow!("bls sig parse: {e:?}"))?;
    match sig.verify(true, message, dst, &[], &pk, true) {
        BLST_ERROR::BLST_SUCCESS => Ok(()),
        err => Err(anyhow!("bls verification failed: {err:?}")),
    }
}

pub fn fast_aggregate_verify(
    message: &[u8],
    public_keys: &[BlsPublicKey],
    signature: &BlsSignature,
    dst: &[u8],
) -> Result<()> {
    if public_keys.is_empty() {
        return Err(anyhow!("missing public keys for aggregate verify"));
    }
    let sig = Signature::from_bytes(&signature.0).map_err(|e| anyhow!("bls sig parse: {e:?}"))?;
    let mut pks = Vec::with_capacity(public_keys.len());
    for pk_bytes in public_keys {
        pks.push(PublicKey::from_bytes(&pk_bytes.0).map_err(|e| anyhow!("bls pk parse: {e:?}"))?);
    }
    let pk_refs: Vec<&PublicKey> = pks.iter().collect();
    match sig.fast_aggregate_verify(true, message, dst, &pk_refs) {
        BLST_ERROR::BLST_SUCCESS => Ok(()),
        err => Err(anyhow!("bls fast aggregate verify failed: {err:?}")),
    }
}

pub fn aggregate_signatures(signatures: &[BlsSignature]) -> Result<BlsSignature> {
    if signatures.is_empty() {
        return Err(anyhow!("no signatures to aggregate"));
    }
    let mut sigs = Vec::with_capacity(signatures.len());
    for sig_bytes in signatures {
        let sig =
            Signature::from_bytes(&sig_bytes.0).map_err(|e| anyhow!("bls sig parse: {e:?}"))?;
        sigs.push(sig);
    }
    let sig_refs: Vec<&Signature> = sigs.iter().collect();
    let agg = AggregateSignature::aggregate(&sig_refs, true)
        .map_err(|e| anyhow!("aggregate signature: {e:?}"))?;
    Ok(BlsSignature(agg.to_signature().to_bytes()))
}

pub fn aggregate_public_keys(keys: &[BlsPublicKey]) -> Result<BlsPublicKey> {
    if keys.is_empty() {
        return Err(anyhow!("no public keys to aggregate"));
    }
    let mut pks = Vec::with_capacity(keys.len());
    for key_bytes in keys {
        let pk = PublicKey::from_bytes(&key_bytes.0).map_err(|e| anyhow!("bls pk parse: {e:?}"))?;
        pks.push(pk);
    }
    let pk_refs: Vec<&PublicKey> = pks.iter().collect();
    let agg = AggregatePublicKey::aggregate(&pk_refs, true)
        .map_err(|e| anyhow!("aggregate pk: {e:?}"))?;
    Ok(BlsPublicKey(agg.to_public_key().to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() -> Result<()> {
        let (sk, pk) = generate_keypair()?;
        let msg = b"hello-receipt";
        let sig = sign(msg, &sk, DST_RECEIPT)?;
        verify(msg, &pk, &sig, DST_RECEIPT)
    }

    #[test]
    fn aggregate_verify_roundtrip() -> Result<()> {
        let (sk1, pk1) = generate_keypair()?;
        let (sk2, pk2) = generate_keypair()?;
        let msg = b"aggregate-receipt";
        let sig1 = sign(msg, &sk1, DST_RECEIPT)?;
        let sig2 = sign(msg, &sk2, DST_RECEIPT)?;
        let agg_sig = aggregate_signatures(&[sig1, sig2])?;
        fast_aggregate_verify(msg, &[pk1, pk2], &agg_sig, DST_RECEIPT)
    }
}
