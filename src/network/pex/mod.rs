//! Peer exchange (PEX) protocol and helpers.
//!
//! PEX lets nodes share peer contact information (PeerId + multiaddrs) over an authenticated
//! transport (Noise/TLS at the libp2p layer). All received data is treated as untrusted and is
//! validated + rate-limited by the caller.

use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

pub const PEX_PROTOCOL: &str = "/onvm/pex/1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexRequest {
    pub want: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexResponse {
    pub peers: Vec<PexPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexPeer {
    pub peer_id: String,
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PexPolicy {
    pub max_peers_per_response: usize,
    pub max_addrs_per_peer: usize,
    pub allow_private_addrs: bool,
    pub allow_loopback_addrs: bool,
}

impl Default for PexPolicy {
    fn default() -> Self {
        Self {
            max_peers_per_response: 128,
            max_addrs_per_peer: 8,
            allow_private_addrs: true,
            allow_loopback_addrs: false,
        }
    }
}

pub fn build_response(
    known: &HashMap<PeerId, HashSet<Multiaddr>>,
    policy: &PexPolicy,
    want: usize,
    self_peer: PeerId,
    requester: PeerId,
) -> PexResponse {
    let mut peers = Vec::new();

    let limit = want.min(policy.max_peers_per_response);

    let mut rng = rand::thread_rng();
    let mut candidates: Vec<_> = known.iter().collect();
    candidates.shuffle(&mut rng);

    for (peer, addrs) in candidates {
        if peers.len() >= limit {
            break;
        }
        if *peer == self_peer {
            continue;
        }
        if *peer == requester {
            continue;
        }

        let mut addr_out = Vec::new();
        for addr in addrs.iter().take(policy.max_addrs_per_peer) {
            if !addr_allowed(
                addr,
                policy.allow_private_addrs,
                policy.allow_loopback_addrs,
            ) {
                continue;
            }
            addr_out.push(addr.to_string());
        }
        if addr_out.is_empty() {
            continue;
        }

        peers.push(PexPeer {
            peer_id: peer.to_string(),
            addrs: addr_out,
        });
    }

    PexResponse { peers }
}

pub fn parse_response(
    resp: PexResponse,
    policy: &PexPolicy,
    self_peer: PeerId,
) -> Vec<(PeerId, Vec<Multiaddr>)> {
    let mut out = Vec::new();
    for peer in resp.peers.into_iter().take(policy.max_peers_per_response) {
        let Ok(peer_id) = PeerId::from_str(&peer.peer_id) else {
            continue;
        };
        if peer_id == self_peer {
            continue;
        }

        let mut addrs = Vec::new();
        for raw in peer.addrs.into_iter().take(policy.max_addrs_per_peer) {
            let Ok(mut addr) = Multiaddr::from_str(&raw) else {
                continue;
            };
            addr = normalize_p2p_component(addr, &peer_id);
            if !addr_allowed(
                &addr,
                policy.allow_private_addrs,
                policy.allow_loopback_addrs,
            ) {
                continue;
            }
            addrs.push(addr);
        }
        if addrs.is_empty() {
            continue;
        }
        out.push((peer_id, addrs));
    }
    out
}

fn normalize_p2p_component(mut addr: Multiaddr, peer_id: &PeerId) -> Multiaddr {
    if let Some(last) = addr.pop() {
        if !matches!(last, libp2p::multiaddr::Protocol::P2p(_)) {
            addr.push(last);
        }
    }
    addr.push(libp2p::multiaddr::Protocol::P2p((*peer_id).into()));
    addr
}

fn addr_allowed(addr: &Multiaddr, allow_private: bool, allow_loopback: bool) -> bool {
    for proto in addr.iter() {
        match proto {
            libp2p::multiaddr::Protocol::Ip4(ip) => {
                if ip.is_unspecified() || ip.is_multicast() {
                    return false;
                }
                if !allow_loopback && ip.is_loopback() {
                    return false;
                }
                if !allow_private && is_ipv4_private_or_link_local(ip) {
                    return false;
                }
            }
            libp2p::multiaddr::Protocol::Ip6(ip) => {
                if ip.is_unspecified() || ip.is_multicast() {
                    return false;
                }
                if !allow_loopback && ip.is_loopback() {
                    return false;
                }
                if !allow_private && is_ipv6_private_or_link_local(ip) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn is_ipv4_private_or_link_local(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

fn is_ipv6_private_or_link_local(ip: Ipv6Addr) -> bool {
    matches!(ip.segments()[0] & 0xfe00, 0xfc00) // unique local (fc00::/7)
        || matches!(ip.segments()[0] & 0xffc0, 0xfe80) // link-local (fe80::/10)
}

pub fn extract_ip(addr: &Multiaddr) -> Option<IpAddr> {
    for proto in addr.iter() {
        match proto {
            libp2p::multiaddr::Protocol::Ip4(ip) => return Some(IpAddr::V4(ip)),
            libp2p::multiaddr::Protocol::Ip6(ip) => return Some(IpAddr::V6(ip)),
            _ => {}
        }
    }
    None
}

pub fn is_public_addr(addr: &Multiaddr) -> bool {
    addr_allowed(addr, false, false)
}

pub fn validate_peer_addrs(
    peer_id: &PeerId,
    addrs: &[Multiaddr],
    policy: &PexPolicy,
) -> Result<Vec<Multiaddr>> {
    let mut unique = HashSet::new();
    let mut out = Vec::new();
    for addr in addrs.iter().take(policy.max_addrs_per_peer) {
        let addr = normalize_p2p_component(addr.clone(), peer_id);
        if !addr_allowed(
            &addr,
            policy.allow_private_addrs,
            policy.allow_loopback_addrs,
        ) {
            continue;
        }
        if unique.insert(addr.to_string()) {
            out.push(addr);
        }
    }
    Ok(out)
}

pub fn rewrite_unspecified_listen_addrs(
    listen_addrs: Vec<Multiaddr>,
    observed_addr: &Multiaddr,
) -> Vec<Multiaddr> {
    let Some(observed_ip) = extract_ip(observed_addr) else {
        return listen_addrs;
    };

    listen_addrs
        .into_iter()
        .map(|addr| rewrite_addr_ip_if_unspecified(addr, observed_ip))
        .collect()
}

fn rewrite_addr_ip_if_unspecified(addr: Multiaddr, observed_ip: IpAddr) -> Multiaddr {
    let mut out = Multiaddr::empty();
    for proto in addr.iter() {
        match (proto, observed_ip) {
            (libp2p::multiaddr::Protocol::Ip4(ip), IpAddr::V4(observed)) if ip.is_unspecified() => {
                out.push(libp2p::multiaddr::Protocol::Ip4(observed));
            }
            (libp2p::multiaddr::Protocol::Ip6(ip), IpAddr::V6(observed)) if ip.is_unspecified() => {
                out.push(libp2p::multiaddr::Protocol::Ip6(observed));
            }
            (other, _) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_trailing_p2p_component() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();

        let addr = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/1234/p2p/{peer_a}")).unwrap();
        let normalized = normalize_p2p_component(addr, &peer_b);

        let last = normalized.iter().last().unwrap();
        assert!(matches!(last, libp2p::multiaddr::Protocol::P2p(p) if p == peer_b));
    }

    #[test]
    fn rejects_unspecified_ips_even_when_private_allowed() {
        let peer = PeerId::random();
        let policy = PexPolicy {
            allow_private_addrs: true,
            allow_loopback_addrs: true,
            ..Default::default()
        };

        let addrs = vec![Multiaddr::from_str("/ip4/0.0.0.0/tcp/37000").unwrap()];
        let validated = validate_peer_addrs(&peer, &addrs, &policy).unwrap();
        assert!(validated.is_empty());
    }

    #[test]
    fn rewrites_unspecified_listen_addrs_using_observed_ip() {
        let listen = vec![Multiaddr::from_str("/ip4/0.0.0.0/tcp/37000").unwrap()];
        let observed = Multiaddr::from_str("/ip4/192.168.1.10/tcp/54321").unwrap();

        let rewritten = rewrite_unspecified_listen_addrs(listen, &observed);
        assert_eq!(rewritten.len(), 1);
        assert_eq!(
            rewritten[0].to_string(),
            "/ip4/192.168.1.10/tcp/37000".to_string()
        );
    }

    #[test]
    fn parse_response_overwrites_mismatched_p2p_component() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let policy = PexPolicy {
            allow_private_addrs: true,
            allow_loopback_addrs: true,
            ..Default::default()
        };

        let resp = PexResponse {
            peers: vec![PexPeer {
                peer_id: peer_b.to_string(),
                addrs: vec![format!("/ip4/127.0.0.1/tcp/1234/p2p/{peer_a}")],
            }],
        };

        let parsed = parse_response(resp, &policy, PeerId::random());
        assert_eq!(parsed.len(), 1);
        let addr = &parsed[0].1[0];
        let last = addr.iter().last().unwrap();
        assert!(matches!(last, libp2p::multiaddr::Protocol::P2p(p) if p == peer_b));
    }
}
