use anyhow::{anyhow, Result};
use blst::min_sig::{AggregatePublicKey, AggregateSignature, PublicKey, SecretKey, Signature};
use blst::BLST_ERROR;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const DST_RECEIPT: &[u8] = b"ONVM:RECEIPT:V1";
pub const DST_COMMITTEE: &[u8] = b"ONVM:COMMITTEE:V1";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsPublicKey(pub [u8; 96]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsSignature(pub [u8; 48]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlsSecretKey(pub [u8; 32]);

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
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        if bytes.len() != 96 {
            return Err(D::Error::custom("invalid bls public key length"));
        }
        let mut arr = [0u8; 96];
        arr.copy_from_slice(&bytes);
        Ok(BlsPublicKey(arr))
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
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        if bytes.len() != 48 {
            return Err(D::Error::custom("invalid bls signature length"));
        }
        let mut arr = [0u8; 48];
        arr.copy_from_slice(&bytes);
        Ok(BlsSignature(arr))
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
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(D::Error::custom("invalid bls secret key length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(BlsSecretKey(arr))
    }
}

pub fn generate_keypair() -> Result<(BlsSecretKey, BlsPublicKey)> {
    let mut ikm = [0u8; 32];
    OsRng.fill_bytes(&mut ikm);
    let sk = SecretKey::key_gen(&ikm, &[]).map_err(|e| anyhow!("bls keygen: {e:?}"))?;
    let pk = sk.sk_to_pk();
    Ok((BlsSecretKey(sk.to_bytes()), BlsPublicKey(pk.to_bytes())))
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
