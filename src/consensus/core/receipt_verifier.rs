//! Aggregated receipt verification (BLS fast aggregate verify + bitmap selection).

use crate::crypto::bls::{fast_aggregate_verify, DST_RECEIPT};
use crate::types::{AggregatedReceipt, CommitteeCertificate};
use anyhow::{bail, Context, Result};

pub struct ReceiptVerifier;

const MAX_COMMITTEE_SIZE: usize = 1000;
const MAX_BITMAP_SIZE: usize = 128;

impl ReceiptVerifier {
    pub fn verify_aggregated_receipt(
        receipt: &AggregatedReceipt,
        committee: &CommitteeCertificate,
    ) -> Result<()> {
        if receipt.receipt.program_id != committee.program_id {
            bail!("program mismatch between receipt and committee");
        }
        if receipt.committee_epoch != committee.epoch {
            bail!("committee epoch mismatch");
        }

        if committee.members.len() > MAX_COMMITTEE_SIZE {
            bail!(
                "committee size exceeds maximum allowed ({})",
                MAX_COMMITTEE_SIZE
            );
        }

        let expected_bitmap_size = (committee.members.len() + 7) / 8;
        if receipt.signer_bitmap.len() > MAX_BITMAP_SIZE
            || receipt.signer_bitmap.len() < expected_bitmap_size
        {
            bail!(
                "invalid signer bitmap size: expected {} bytes, got {}",
                expected_bitmap_size,
                receipt.signer_bitmap.len()
            );
        }

        let (signer_keys, signer_weight) =
            select_signers(committee, &receipt.signer_bitmap).context("extracting signer keys")?;
        if signer_keys.is_empty() || signer_weight == 0 {
            bail!("no signers present in aggregate receipt");
        }
        if signer_weight < committee.threshold as u64 {
            bail!(
                "insufficient signer weight for aggregate receipt (have {}, need {})",
                signer_weight,
                committee.threshold
            );
        }

        let receipt_id = receipt.receipt.id();
        fast_aggregate_verify(
            &receipt_id,
            &signer_keys,
            &receipt.aggregate_signature,
            DST_RECEIPT,
        )
        .context("aggregate receipt verification failed")
    }
}

fn select_signers(
    committee: &CommitteeCertificate,
    bitmap: &[u8],
) -> Result<(Vec<crate::crypto::bls::BlsPublicKey>, u64)> {
    let expected_size = (committee.members.len() + 7) / 8;
    if bitmap.len() < expected_size {
        bail!(
            "bitmap too small: expected at least {} bytes for {} members, got {}",
            expected_size,
            committee.members.len(),
            bitmap.len()
        );
    }

    let mut selected = Vec::new();
    let mut weight = 0u64;
    for (idx, member) in committee.members.iter().enumerate() {
        let byte_index = idx / 8;
        let bit_index = idx % 8;
        if byte_index >= bitmap.len() {
            break;
        }
        if (bitmap[byte_index] >> bit_index) & 1 == 1 {
            selected.push(member.bls_public_key.clone());
            weight = weight.saturating_add(member.weight);
        }
    }
    Ok((selected, weight))
}
