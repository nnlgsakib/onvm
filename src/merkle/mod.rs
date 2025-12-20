use crate::crypto::hashing::hash_bytes;

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
}
