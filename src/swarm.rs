use futures::StreamExt;
use libp2p::{
    identity,
    kad::{store::MemoryStore, Behaviour as Kademlia, Config as KademliaConfig, RecordKey as Key},
    request_response::OutboundRequestId,
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm, SwarmBuilder, StreamProtocol,
};

use crate::behaviour::{FileRequest, MyBehaviour};

pub struct ManagedSwarm(pub Swarm<MyBehaviour>);

impl ManagedSwarm {
    pub async fn new(addr: Multiaddr, bootstrap_nodes: Vec<Multiaddr>) -> Self {
        let mut swarm = create_swarm(bootstrap_nodes).await;
        swarm.listen_on(addr).unwrap();

        Self(swarm)
    }

    pub async fn bootstrap(&mut self) {
        let _ = self.0.behaviour_mut().kademlia.bootstrap();
    }

    pub fn get(&mut self, key: Key) -> libp2p::kad::QueryId {
        self.0.behaviour_mut().kademlia.get_record(key)
    }

    pub fn put(&mut self, key: Key, value: Vec<u8>) -> libp2p::kad::QueryId {
        let record = libp2p::kad::Record {
            key,
            value,
            publisher: None,
            expires: None,
        };

        self.0
            .behaviour_mut()
            .kademlia
            .put_record(record)
            .expect("Failed to put record locally")
    }

    pub fn start_providing(&mut self, key: Key) -> libp2p::kad::QueryId {
        self.0
            .behaviour_mut()
            .kademlia
            .start_providing(key)
            .expect("Failed to start providing key")
    }

    pub fn get_providers(&mut self, key: Key) -> libp2p::kad::QueryId {
        self.0.behaviour_mut().kademlia.get_providers(key)
    }

    pub async fn send_request(
        &mut self,
        peer: PeerId,
        request: FileRequest,
    ) -> Result<OutboundRequestId, String> {
        Ok(self.0.behaviour_mut().request_response.send_request(&peer, request))
    }
}

async fn create_swarm(bootstrap_nodes: Vec<Multiaddr>) -> Swarm<MyBehaviour> {
    SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            Default::default(),
            (libp2p::tls::Config::new, libp2p::noise::Config::new),
            libp2p::yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(move |key| {
            let local_peer_id = PeerId::from(key.public());
            let store = MemoryStore::new(local_peer_id);
            let mut kademlia = Kademlia::with_config(
                local_peer_id,
                store,
                KademliaConfig::new(StreamProtocol::new("/ipfs/kad/1.0.0")),
            );

            for addr in bootstrap_nodes {
                kademlia.add_address(&local_peer_id, addr);
            }

            let mdns = libp2p::mdns::tokio::Behaviour::new(
                libp2p::mdns::Config::default(),
                local_peer_id,
            )
            .unwrap();

            MyBehaviour {
                kademlia,
                mdns,
                request_response: MyBehaviour::create_req_res(),
            }
        })
        .unwrap()
        .build()
}
