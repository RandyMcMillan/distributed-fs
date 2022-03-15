use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     KademliaEvent,
     QueryResult,
     PeerRecord, 
     Record,
     QueryId
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    NetworkBehaviour, 
    swarm::{NetworkBehaviourEventProcess}
};
use std::str;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use crate::entry::Entry;

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
pub struct MyBehaviour {
	pub kademlia: Kademlia<MemoryStore>,
	pub mdns: Mdns,
	#[behaviour(ignore)]
	pub users: Arc<Mutex<HashMap<QueryId, String>>>
}	

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
	fn inject_event(&mut self, event: MdnsEvent) {
		if let MdnsEvent::Discovered(list) = event {
			for (peer_id, multiaddr) in list {
				println!("{:?}, {:?}", peer_id, multiaddr);
				self.kademlia.add_address(&peer_id, multiaddr);
			}
		}
	}
}

impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
	fn inject_event(&mut self, msg: KademliaEvent) {
		match msg {
			KademliaEvent::OutboundQueryCompleted { result, id, .. } => match result {
				QueryResult::GetRecord(Ok(ok)) => {
					for PeerRecord {
						record: Record { key, value, .. },
						..
					} in ok.records {
						let entry: Entry = serde_json::from_str(&str::from_utf8(&value).unwrap()).unwrap();
						let username = self.users.lock().unwrap().remove(&id).unwrap();

						if entry.user == username || entry.public || entry.read_users.contains(&username)  {
							println!("{:?}", entry);
						} else {
							println!("Read access to {:?} not allowed", key);
						}
					}
				},
				QueryResult::GetRecord(Err(err)) => {
					eprintln!("Failed to get record: {:?}", err);
					self.users.lock().unwrap().remove(&id).unwrap();
				},
				_ => {
					println!("\n{:?}\n", result);
				}
			},
			_ => {}
		}
	}
}
