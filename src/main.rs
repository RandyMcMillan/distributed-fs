use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     KademliaEvent
};
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    NetworkBehaviour, 
    PeerId, 
    Swarm,
    swarm::{NetworkBehaviourEventProcess, SwarmEvent}
};
use async_std::{task, io};
use futures::{prelude::*};
use std::error::Error;


async fn create_swarm() -> Swarm<MyBehaviour> {
	let local_key = identity::Keypair::generate_ed25519();
	let local_peer_id = PeerId::from(local_key.public());

	let transport = development_transport(local_key).await.unwrap();

	let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = task::block_on(Mdns::new(MdnsConfig::default())).unwrap();
        let behaviour = MyBehaviour { kademlia, mdns };
        Swarm::new(transport, behaviour, local_peer_id)
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
struct MyBehaviour {
	kademlia: Kademlia<MemoryStore>,
	mdns: Mdns,
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
	fn inject_event(&mut self, event: MdnsEvent) {
		// println!("MDNS event, {:?}", event);
		if let MdnsEvent::Discovered(list) = event {
			for (peer_id, multiaddr) in list {
				println!("{:?}, {:?}", peer_id, multiaddr);
				// self.kademlia.add_address(&peer_id, multiaddr);
			}
		}
	}
}

impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
	fn inject_event(&mut self, msg: KademliaEvent) {
		println!("{:?}", msg);
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let mut swarm = create_swarm().await;

	let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

	loop {
		tokio::select! {
			line = stdin.select_next_some() => {
				println!("{}", line.expect("stdin closed"))
			},
			event = swarm.select_next_some() => match event {
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Listening in {:?}", address);
				},
				_ => {}
			}
		}
	}
}
