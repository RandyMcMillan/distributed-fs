use std::fs;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     record::Key,
     Quorum,
     Record
};
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
    swarm::{ SwarmEvent}
};
use async_std::{task, io};
use futures::{prelude::*};
use std::error::Error;

mod entry;
mod behaviour;

use entry::Entry;
use behaviour::MyBehaviour;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let username = fs::read_to_string("user")?;

	let mut swarm = create_swarm().await;

	let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;

	loop {
		tokio::select! {
			line = stdin.select_next_some() => {
				handle_input(
					&mut swarm.behaviour_mut().kademlia,
					line.expect("stdin closed"),
					&username
				);
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

fn handle_input(kad: &mut Kademlia<MemoryStore>, line: String, username: &str) {
	let mut args = line.split(' '); 

	match args.next() {
		Some("GET") => {
			let key = {
				match args.next() {
					Some(key) => Key::new(&key),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};
			kad.get_record(&key, Quorum::One);
		},
		Some("PUT") => {
			let path = {
				match args.next() {
					Some(key) => key.to_string(),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};	

			let public = {
				match args.next() {
					Some(value) => value == "true",
					None => {
						eprintln!("Expected value");
						return;
					}
				}
			};

			let read_users: Vec<String> = if public { Vec::new() } else { args.map(|s| s.to_string()).collect() };

			let new_entry = Entry {
				path,
				user: username.to_string(),
				public,
				read_users 
			};

			let value = serde_json::to_vec(&new_entry).unwrap();

			let record = Record {
				key: Key::new(&username),
				value,
				publisher: None,
				expires: None,
			};

			kad
				.put_record(record, Quorum::One)
				.expect("Failed to store record locally.");
		},
		Some("GET_PROVIDERS") => {
			let key = {
				match args.next() {
					Some(key) => Key::new(&key),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};

			kad.get_providers(key);
		},
		Some("PUT_PROVIDER") => {
			let key = {
				match args.next() {
					Some(key) => Key::new(&key),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};

			kad
				.start_providing(key)
				.expect("Failed to start providing key");
		},
		_ => {}
	}
}
