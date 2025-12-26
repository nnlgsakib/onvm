//! Peer exchange (PEX) protocol and helpers.
//!
//! PEX lets nodes share peer contact information (PeerId + multiaddrs) over an authenticated
//! transport (Noise/TLS at the libp2p layer). All received data is treated as untrusted and is
//! validated + rate-limited by the caller.

use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
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
) -> PexResponse {
    let mut peers = Vec::new();

    let limit = want
        .min(policy.max_peers_per_response)
        .min(known.len().saturating_sub(1));

    for (peer, addrs) in known.iter() {
        if peers.len() >= limit {
            break;
        }
        if *peer == self_peer {
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
            addr = ensure_p2p_component(addr, &peer_id);
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

fn ensure_p2p_component(mut addr: Multiaddr, peer_id: &PeerId) -> Multiaddr {
    let has_p2p = addr
        .iter()
        .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)));
    if has_p2p {
        return addr;
    }
    addr.push(libp2p::multiaddr::Protocol::P2p((*peer_id).into()));
    addr
}

fn addr_allowed(addr: &Multiaddr, allow_private: bool, allow_loopback: bool) -> bool {
    for proto in addr.iter() {
        match proto {
            libp2p::multiaddr::Protocol::Ip4(ip) => {
                if !allow_loopback && ip.is_loopback() {
                    return false;
                }
                if !allow_private && is_ipv4_private(ip) {
                    return false;
                }
            }
            libp2p::multiaddr::Protocol::Ip6(ip) => {
                if !allow_loopback && ip.is_loopback() {
                    return false;
                }
                if !allow_private && is_ipv6_private(ip) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn is_ipv4_private(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast()
}

fn is_ipv6_private(ip: Ipv6Addr) -> bool {
    ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_loopback()
        || matches!(ip.segments()[0] & 0xfe00, 0xfc00) // unique local (fc00::/7)
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

pub fn validate_peer_addrs(
    peer_id: &PeerId,
    addrs: &[Multiaddr],
    policy: &PexPolicy,
) -> Result<Vec<Multiaddr>> {
    let mut unique = HashSet::new();
    let mut out = Vec::new();
    for addr in addrs.iter().take(policy.max_addrs_per_peer) {
        let addr = ensure_p2p_component(addr.clone(), peer_id);
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
