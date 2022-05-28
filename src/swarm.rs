use libp2p::{
	Swarm,
	swarm::SwarmEvent,
	development_transport, 
	identity,
	mdns::{Mdns, MdnsConfig},
	PeerId, 
};
use libp2p::request_response::{
	ResponseChannel,
	RequestId,
	RequestResponseEvent,
	RequestResponseMessage
};
use libp2p::kad::{
	Record, 
	Quorum,
	KademliaEvent,
	QueryResult,
	record::Key
};
use futures::StreamExt;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::Kademlia;
use async_std::task;
use futures::channel::{oneshot};

use crate::behaviour::{
	MyBehaviour, 
	OutEvent, 
	FileRequest, 
	FileRequestType,
	FileResponse
};

pub struct ManagedSwarm (pub Swarm<MyBehaviour>);

impl ManagedSwarm {
	pub async fn new(addr: &str) -> Self {
		let mut swarm = create_swarm().await;
		swarm.listen_on(addr.parse().unwrap()).unwrap();

		Self(swarm)
	}
 
	pub async fn get(&mut self, key: &Key) -> Result<Record, &str> {
		let behaviour = self.0.behaviour_mut();

		behaviour.kademlia.get_record(&key, Quorum::One);

		let res = loop {
			if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted { result, .. })) = self.0.select_next_some().await {
				break result;
			}
		};

		match res {
			QueryResult::GetRecord(Ok(ok)) => Ok(ok.records.get(0).unwrap().record.clone()),
			_ => {
				println!("{:?}", res);
				Err("Record not found")
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
		behaviour.kademlia.put_record(record.clone(), Quorum::One).expect("Failed to put record locally");

		let res = loop {
			if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {result, .. })) = self.0.select_next_some().await {
				break result;
			}
		};

		match res {
			QueryResult::PutRecord(d) => {
				match d {
					Ok(dd) => {
						// let behaviour = self.0.behaviour_mut();

						// behaviour.kademlia.get_providers(dd.key.clone());

						// let res = loop {
						// 	if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {result, .. })) = self.0.select_next_some().await {
						// 		break result;
						// 	}
						// };

						// let peer_id = match res {
						// 	QueryResult::GetProviders(p) => {
						// 		let p = p.unwrap();
						// 		if p.closest_peers.len() == 0 {
						// 			return Ok(dd.key);
						// 		}

						// 		p.closest_peers[0]
						// 	},
						// 	_ => {
						// 		return Ok(dd.key.clone());
						// 	}
						// };
						
						// let behaviour = self.0.behaviour_mut();
						// let key = String::from_utf8(dd.key.clone().to_vec()).unwrap();
						// behaviour
						// 	.request_response
						// 	.send_request(&peer_id, FileRequest(FileRequestType::ProvideRequest(key)));

						Ok(dd.key)
					},
					Err(e) => Err(format!("{:?}", e))
				}
			}
			_ => Err("Something went wrong".to_string())
		}
	}

	pub async fn start_providing(&mut self, key: Key) -> Result<Key, String> {
		let behaviour = self.0.behaviour_mut();

		behaviour.kademlia.start_providing(key.clone()).expect("Failed to start providing key");

		let res = loop {
			if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted { result, .. })) = self.0.select_next_some().await {
				break result;
			}
		};

		match res {
			QueryResult::StartProviding(r) => match r {
				Ok(_r) => Ok(key),
				Err(_error) => Err("Error on StartProviding".to_string())
			},
			_ => Err("Something went wrong".to_string())
		}
	}

	pub async fn get_providers(&mut self, key: Key) -> Result<Vec<PeerId>, String> {
		let behaviour = self.0.behaviour_mut();

		behaviour.kademlia.get_providers(key);

		let res = loop {
			if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {result, .. })) = self.0.select_next_some().await {
				break result;
			}
		};

		match res {
			QueryResult::GetProviders(r) => match r {
				Ok(providers) => Ok(providers.closest_peers),
				Err(_error) => Err("Error on GetProviders".to_string())
			},
			_ => Err("Something went wrong".to_string())
		}
	}

	pub async fn send_request(&mut self, peer: PeerId, request: FileRequest) -> Result<FileResponse, String> {
		let behaviour = self.0.behaviour_mut();

		behaviour
			.request_response
			.send_request(&peer, request);

		let res = loop {
			if let SwarmEvent::Behaviour(
				OutEvent::RequestResponse(
					RequestResponseEvent::Message { message: RequestResponseMessage::Response { response, .. }, .. }
				)
			) = self.0.select_next_some().await {
				break response;
			}
		};

		Ok(res)
	}

	pub async fn send_response(&mut self, response: FileResponse, channel: ResponseChannel<FileResponse>) -> Result<(), String> {
		let behaviour = self.0.behaviour_mut();

		match behaviour
			.request_response
			.send_response(channel, response) {
				Ok(()) => {},
				Err(error) => println!("Error while sending res: {:?}", error)
		};

		Ok(())
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
		request_response: MyBehaviour::create_req_res()
	};
        Swarm::new(transport, behaviour, local_peer_id)
}