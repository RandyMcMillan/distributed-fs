use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     KademliaEvent,
     QueryResult,
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    NetworkBehaviour, 
    swarm::{NetworkBehaviourEventProcess}
};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent", event_process = false)]
pub struct MyBehaviour {
	pub kademlia: Kademlia<MemoryStore>,
	pub mdns: Mdns,
}	

#[derive(Debug)]
pub enum OutEvent {
    Kademlia(KademliaEvent),
    Mdns(MdnsEvent)
}

impl From<KademliaEvent> for OutEvent {
	fn from(event: KademliaEvent) -> Self {
		println!("{:?}", event);
		Self::Kademlia(event)
	}
}

impl From<MdnsEvent> for OutEvent {
	fn from(event: MdnsEvent) -> Self {
		Self::Mdns(event)
	}
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
			KademliaEvent::OutboundQueryCompleted { result, .. } => match result {
				QueryResult::GetRecord(Ok(_ok)) => {},
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
