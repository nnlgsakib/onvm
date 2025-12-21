use crate::crypto::bls::{fast_aggregate_verify, DST_RECEIPT};
use crate::types::{AggregatedReceipt, CommitteeCertificate};
use anyhow::{bail, Context, Result};

pub struct ReceiptVerifier;

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

        let signer_keys =
            select_signers(committee, &receipt.signer_bitmap).context("extracting signer keys")?;
        if signer_keys.is_empty() {
            bail!("no signers present in aggregate receipt");
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
) -> Result<Vec<crate::crypto::bls::BlsPublicKey>> {
    let mut selected = Vec::new();
    for (idx, member) in committee.members.iter().enumerate() {
        let byte_index = idx / 8;
        let bit_index = idx % 8;
        if byte_index >= bitmap.len() {
            break;
        }
        if (bitmap[byte_index] >> bit_index) & 1 == 1 {
            selected.push(member.bls_public_key.clone());
        }
    }
    Ok(selected)
}
