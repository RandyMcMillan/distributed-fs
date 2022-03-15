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
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

mod entry;
mod behaviour;

use entry::{Entry, Children};
use behaviour::MyBehaviour;

async fn create_swarm() -> Swarm<MyBehaviour> {
	let local_key = identity::Keypair::generate_ed25519();
	let local_peer_id = PeerId::from(local_key.public());

	let transport = development_transport(local_key).await.unwrap();

	let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = task::block_on(Mdns::new(MdnsConfig::default())).unwrap();
        let behaviour = MyBehaviour { 
		kademlia, 
		mdns, 
		users: Arc::new(Mutex::new(HashMap::new()))
	};
        Swarm::new(transport, behaviour, local_peer_id)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let mut swarm = create_swarm().await;

	let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;

	loop {
		tokio::select! {
			line = stdin.select_next_some() => {
				handle_input(
					&mut swarm.behaviour_mut(),
					line.expect("stdin closed"),
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

fn handle_input(behaviour: &mut MyBehaviour, line: String) {
	let mut args = line.split(' '); 

	let kad = &mut behaviour.kademlia;

	match args.next() {
		Some("GET") => {
			let hash = {
				match args.next() {
					Some(key) => Key::new(&key),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};

			let username = {
				match args.next() {
					Some(name) => name.to_string(),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};	

			let query_id = kad.get_record(&hash, Quorum::One);
			behaviour.users.lock().unwrap().insert(query_id, String::from(username));
		},
		Some("PUT") => {
			let name = {
				match args.next() {
					Some(name) => name.to_string(),
					None => {
						eprintln!("Expected key");
						return;
					}
				}
			};	

			let username = {
				match args.next() {
					Some(name) => name.to_string(),
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
				name,
				user: username.to_string(),
				public,
				read_users,
				children: Vec::<Children>::new()
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
