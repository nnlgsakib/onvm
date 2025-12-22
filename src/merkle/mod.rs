use crate::crypto::hashing::hash_bytes;
use std::collections::HashMap;

/// Compute a Merkle root from key/value pairs using deterministic ordering.
pub fn merkle_root(pairs: &[(Vec<u8>, Vec<u8>)]) -> [u8; 32] {
    if pairs.is_empty() {
        return hash_bytes(b"onvm-empty-root");
    }

    let mut leaves: Vec<[u8; 32]> = pairs
        .iter()
        .map(|(k, v)| hash_pair(hash_bytes(k), hash_bytes(v)))
        .collect();
    leaves.sort(); // deterministic ordering
    reduce(leaves)
}

fn hash_pair(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    hash_bytes([a.as_slice(), b.as_slice()].concat().as_slice())
}

fn reduce(mut nodes: Vec<[u8; 32]>) -> [u8; 32] {
    while nodes.len() > 1 {
        if nodes.len() % 2 == 1 {
            let last = *nodes.last().unwrap();
            nodes.push(last);
        }
        let mut next = Vec::with_capacity(nodes.len() / 2);
        for chunk in nodes.chunks(2) {
            next.push(hash_pair(chunk[0], chunk[1]));
        }
        nodes = next;
    }
    nodes[0]
}

/// Constant for empty sparse nodes.
pub const SMT_EMPTY: [u8; 32] = [
    b'o', b'n', b'v', b'm', b'-', b's', b'm', b't', b'-', b'e', b'm', b'p', b't', b'y', b'-', b'o',
    b'n', b'v', b'm', b'-', b's', b'm', b't', b'-', b'e', b'm', b'p', b't', b'y', b'!', b'!', 0u8,
];

/// Compute a sparse Merkle tree root (256-bit keyspace) over key/value pairs.
/// Keys are hashed before insertion to avoid structural leakage.
pub fn sparse_merkle_root(pairs: &[(Vec<u8>, Vec<u8>)]) -> [u8; 32] {
    let map = dedup_leaves(pairs);
    if map.is_empty() {
        return SMT_EMPTY;
    }
    let leaves: Vec<([u8; 32], [u8; 32])> = map.into_iter().collect();
    compute_smt_root(0, &leaves)
}

/// Build a proof for a key in the sparse Merkle tree. Returns (root, proof).
pub fn sparse_merkle_proof(pairs: &[(Vec<u8>, Vec<u8>)], key: &[u8]) -> ([u8; 32], Vec<[u8; 32]>) {
    let map = dedup_leaves(pairs);
    let target = hash_bytes(key);
    let leaves: Vec<([u8; 32], [u8; 32])> = map.into_iter().collect();
    let mut proof = Vec::with_capacity(256);
    let root = compute_smt_with_proof(0, &leaves, &target, &mut proof);
    (root, proof)
}

/// Verify a sparse Merkle proof for a single key/value pair.
pub fn verify_sparse_merkle_proof(
    key: &[u8],
    value: &[u8],
    proof: &[[u8; 32]],
    expected_root: [u8; 32],
) -> bool {
    let key_hash = hash_bytes(key);
    let mut node = hash_leaf(&key_hash, &hash_bytes(value));
    for (offset, sibling) in proof.iter().enumerate() {
        // Proof is collected from leaf to root, so walk bits from the least significant upwards.
        let depth = 255usize.saturating_sub(offset);
        let bit = bit_at(&key_hash, depth);
        node = if bit {
            hash_internal(sibling, &node)
        } else {
            hash_internal(&node, sibling)
        };
    }
    node == expected_root
}

fn dedup_leaves(pairs: &[(Vec<u8>, Vec<u8>)]) -> HashMap<[u8; 32], [u8; 32]> {
    let mut map = HashMap::new();
    for (k, v) in pairs {
        let key_hash = hash_bytes(k);
        let val_hash = hash_bytes(v);
        map.insert(key_hash, hash_leaf(&key_hash, &val_hash));
    }
    map
}

fn compute_smt_root(level: usize, leaves: &[([u8; 32], [u8; 32])]) -> [u8; 32] {
    if leaves.is_empty() {
        return SMT_EMPTY;
    }
    if level == 256 {
        // All keys identical; return the leaf hash
        return leaves[0].1;
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (k, v) in leaves {
        if bit_at(k, level) {
            right.push((*k, *v));
        } else {
            left.push((*k, *v));
        }
    }
    let left_hash = compute_smt_root(level + 1, &left);
    let right_hash = compute_smt_root(level + 1, &right);
    hash_internal(&left_hash, &right_hash)
}

fn compute_smt_with_proof(
    level: usize,
    leaves: &[([u8; 32], [u8; 32])],
    target: &[u8; 32],
    proof: &mut Vec<[u8; 32]>,
) -> [u8; 32] {
    if leaves.is_empty() {
        // missing key; use empty path
        proof.extend(std::iter::repeat(SMT_EMPTY).take(256 - level));
        return SMT_EMPTY;
    }
    if level == 256 {
        // reached leaf
        return leaves[0].1;
    }

    let mut left = Vec::new();
    let mut right = Vec::new();
    for (k, v) in leaves {
        if bit_at(k, level) {
            right.push((*k, *v));
        } else {
            left.push((*k, *v));
        }
    }

    let bit = bit_at(target, level);
    let (keep, sibling) = if bit { (right, left) } else { (left, right) };

    let keep_hash = compute_smt_with_proof(level + 1, &keep, target, proof);
    let sibling_hash = compute_smt_root(level + 1, &sibling);
    proof.push(sibling_hash);

    if bit {
        hash_internal(&sibling_hash, &keep_hash)
    } else {
        hash_internal(&keep_hash, &sibling_hash)
    }
}

fn hash_leaf(key_hash: &[u8; 32], value_hash: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(4 + 32 + 32);
    buf.extend_from_slice(b"leaf");
    buf.extend_from_slice(key_hash);
    buf.extend_from_slice(value_hash);
    hash_bytes(&buf)
}

fn hash_internal(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(4 + 32 + 32);
    buf.extend_from_slice(b"node");
    buf.extend_from_slice(left);
    buf.extend_from_slice(right);
    hash_bytes(&buf)
}

fn bit_at(key_hash: &[u8; 32], depth: usize) -> bool {
    let byte = depth / 8;
    let bit = 7 - (depth % 8);
    (key_hash[byte] >> bit) & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_constant() {
        let root = merkle_root(&[]);
        assert_eq!(root, hash_bytes(b"onvm-empty-root"));
    }

    #[test]
    fn ordering_is_deterministic() {
        let a = (b"a".to_vec(), b"1".to_vec());
        let b = (b"b".to_vec(), b"2".to_vec());

        let root_ab = merkle_root(&[a.clone(), b.clone()]);
        let root_ba = merkle_root(&[b, a]);

        assert_eq!(root_ab, root_ba);
    }

    #[test]
    fn sparse_merkle_root_changes_with_value() {
        let root1 = sparse_merkle_root(&[(b"a".to_vec(), b"1".to_vec())]);
        let root2 = sparse_merkle_root(&[(b"a".to_vec(), b"2".to_vec())]);
        assert_ne!(root1, root2);
    }

    #[test]
    fn sparse_merkle_proof_verifies() {
        let pairs = vec![
            (b"alpha".to_vec(), b"one".to_vec()),
            (b"beta".to_vec(), b"two".to_vec()),
        ];
        let (root, proof) = sparse_merkle_proof(&pairs, b"alpha");
        assert!(verify_sparse_merkle_proof(b"alpha", b"one", &proof, root));
        assert!(!verify_sparse_merkle_proof(
            b"alpha", b"wrong", &proof, root
        ));
    }
}
