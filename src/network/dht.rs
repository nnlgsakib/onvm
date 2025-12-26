use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Behaviour as Kademlia, Config as KadConfig};
use libp2p::PeerId;
use std::time::Duration;

pub const PROVIDER_RECORD_TTL: Duration = Duration::from_secs(60 * 60);
pub const PROVIDER_PUBLICATION_INTERVAL: Duration = Duration::from_secs(10 * 60);

pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(60);
pub const ANNOUNCE_AFTER_CONNECT_DELAY: Duration = Duration::from_secs(1);

pub const FIND_PROVIDERS_MAX_WAIT: Duration = Duration::from_secs(5);
pub const FIND_PROVIDERS_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub fn build_kademlia(peer_id: PeerId) -> Kademlia<MemoryStore> {
    let store = MemoryStore::new(peer_id);
    let mut kad_config = KadConfig::default();
    kad_config
        .disjoint_query_paths(true)
        .set_query_timeout(Duration::from_secs(60))
        .set_provider_record_ttl(Some(PROVIDER_RECORD_TTL))
        .set_provider_publication_interval(Some(PROVIDER_PUBLICATION_INTERVAL));
    let mut kademlia = Kademlia::with_config(peer_id, store, kad_config);
    kademlia.set_mode(Some(libp2p::kad::Mode::Server));
    kademlia
}
