use libp2p::{
	Swarm,
	swarm::SwarmEvent,
	PeerId
};
use libp2p::kad::{
	Record, 
	Quorum,
	KademliaEvent,
	QueryResult,
	record::Key
};
use futures::StreamExt;
use crate::behaviour::{MyBehaviour, OutEvent, FileRequest};
use std::str::FromStr;

pub struct Dht (pub Swarm<MyBehaviour>);

impl Dht {
	pub fn new(swarm: Swarm<MyBehaviour>) -> Self {
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
			_ => Err("Record not found")
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
						let behaviour = self.0.behaviour_mut();
						
						let peer = PeerId::from_str("12D3KooWDpAHAhK6B7S45viqEcyVmJR2Hj7vM7AXNSa9rWMFF6BQ").unwrap();
						let request_id = behaviour
							.request_response
							.send_request(&peer, FileRequest("some_file_name".to_string()));
						println!("Request sent: {:?}", request_id);

						Ok(dd.key)
					},
					Err(e) => Err(format!("{:?}", e))
				}
			}
			_ => Err("Something went wrong".to_string())
		}
	}
}