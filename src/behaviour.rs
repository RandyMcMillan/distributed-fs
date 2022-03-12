use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     KademliaEvent,
     QueryResult,
     PeerRecord, 
     Record
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    NetworkBehaviour, 
    swarm::{NetworkBehaviourEventProcess}
};
use std::str;

// pub mod entry;
use crate::entry::Entry;

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
pub struct MyBehaviour {
	pub kademlia: Kademlia<MemoryStore>,
	pub mdns: Mdns,
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
	fn inject_event(&mut self, event: MdnsEvent) {
		// println!("MDNS event, {:?}", event);
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
					} in ok.records
					{
						println!(
							"Got record {:?} {:?}\n{:?}",
							str::from_utf8(key.as_ref()).unwrap(),
							str::from_utf8(&value).unwrap(),
							id
						);

						let entry: Entry = serde_json::from_str(&str::from_utf8(&value).unwrap()).unwrap();
						println!("{:?}", entry);
					}
				},
				QueryResult::GetRecord(Err(err)) => {
					eprintln!("Failed to get record: {:?}", err);
				},
				_ => {
					println!("\n{:?}\n", result);
				}
			},
			_ => {}
		}
	}
}
