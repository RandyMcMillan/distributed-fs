use async_std::task;
use futures::StreamExt;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{record::Key, Kademlia, KademliaEvent, QueryResult, Quorum, Record};
use libp2p::request_response::RequestId;
use libp2p::{
    development_transport, identity,
    mdns::{Mdns, MdnsConfig},
    swarm::SwarmEvent,
    PeerId, Swarm,
};

use crate::behaviour::{FileRequest, MyBehaviour, OutEvent};

pub struct ManagedSwarm(pub Swarm<MyBehaviour>);

impl ManagedSwarm {
    pub async fn new(addr: &str) -> Self {
        let mut swarm = create_swarm().await;
        swarm.listen_on(addr.parse().unwrap()).unwrap();

        Self(swarm)
    }

    pub async fn get(&mut self, key: Key) -> Result<Record, String> {
        let behaviour = self.0.behaviour_mut();

        behaviour.kademlia.get_record(&key, Quorum::One);

        let res = loop {
            if let SwarmEvent::Behaviour(OutEvent::Kademlia(
                KademliaEvent::OutboundQueryCompleted { result, .. },
            )) = self.0.select_next_some().await
            {
                break result;
            }
        };

        match res {
            QueryResult::GetRecord(Ok(ok)) => Ok(ok.records.get(0).unwrap().record.clone()),
            _ => {
                // println!("{:?}", res);
                Err("Record not found".to_owned())
            }
        }
    }

    pub async fn put(&mut self, key: Key, value: Vec<u8>) -> Result<Key, String> {
        let record = Record {
            key,
            value,
            publisher: None,
            expires: None,
        };

        let behaviour = self.0.behaviour_mut();
        behaviour
            .kademlia
            .put_record(record.clone(), Quorum::One)
            .expect("Failed to put record locally");

        let res = loop {
            if let SwarmEvent::Behaviour(OutEvent::Kademlia(
                KademliaEvent::OutboundQueryCompleted { result, .. },
            )) = self.0.select_next_some().await
            {
                break result;
            }
        };

        match res {
            QueryResult::PutRecord(d) => match d {
                Ok(dd) => Ok(dd.key),
                Err(e) => Err(format!("{:?}", e)),
            },
            _ => Err("Something went wrong".to_string()),
        }
    }

    pub async fn start_providing(&mut self, key: Key) -> Result<Key, String> {
        let behaviour = self.0.behaviour_mut();

        behaviour
            .kademlia
            .start_providing(key.clone())
            .expect("Failed to start providing key");

        let res = loop {
            if let SwarmEvent::Behaviour(OutEvent::Kademlia(
                KademliaEvent::OutboundQueryCompleted { result, .. },
            )) = self.0.select_next_some().await
            {
                break result;
            }
        };

        match res {
            QueryResult::StartProviding(r) => match r {
                Ok(_r) => Ok(key),
                Err(_error) => Err("Error on StartProviding".to_string()),
            },
            _ => Err("Something went wrong".to_string()),
        }
    }

    pub async fn get_providers(&mut self, key: Key) -> Result<Vec<PeerId>, String> {
        let behaviour = self.0.behaviour_mut();

        behaviour.kademlia.get_providers(key);

        let res = loop {
            if let SwarmEvent::Behaviour(OutEvent::Kademlia(
                KademliaEvent::OutboundQueryCompleted { result, .. },
            )) = self.0.select_next_some().await
            {
                break result;
            }
        };

        match res {
            QueryResult::GetProviders(r) => match r {
                Ok(providers) => Ok(providers.closest_peers),
                Err(_error) => Err("Error on GetProviders".to_string()),
            },
            _ => Err("Something went wrong".to_string()),
        }
    }

    pub async fn send_request(
        &mut self,
        peer: PeerId,
        request: FileRequest,
    ) -> Result<RequestId, String> {
        let behaviour = self.0.behaviour_mut();

        let req_id = behaviour.request_response.send_request(&peer, request);

        Ok(req_id)
    }
}

async fn create_swarm() -> Swarm<MyBehaviour> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("{:?}", local_peer_id);

    let transport = development_transport(local_key).await.unwrap();

    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::new(local_peer_id, store);
    let mdns = task::block_on(Mdns::new(MdnsConfig::default())).unwrap();
    let behaviour = MyBehaviour {
        kademlia,
        mdns,
        request_response: MyBehaviour::create_req_res(),
    };
    Swarm::new(transport, behaviour, local_peer_id)
}
