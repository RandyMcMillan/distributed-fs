use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     record::Key,
     Quorum,
     Record,
};
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
    swarm::SwarmEvent
};
use async_std::task;
use futures::{prelude::*};
use std::error::Error;
use tokio::net::TcpListener; 
use std::env;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{PublicKey, Secp256k1};

mod entry;
mod behaviour;
mod handler;
mod dht;

use entry::{Entry, Children};
use behaviour::MyBehaviour;
use dht::Dht;

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
	};
        Swarm::new(transport, behaviour, local_peer_id)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let args: Vec<String> = env::args().collect();

	if args.len() > 1 && &args[1] == "gen-keypair" {
		let secp = Secp256k1::new();
		let mut rng = OsRng::new().unwrap();
		let (secret_key, public_key) = secp.generate_keypair(&mut rng);

		assert_eq!(public_key.to_string(), PublicKey::from_secret_key(&secp, &secret_key).to_string());
		println!("Public key: {}\nPrivate Key: {}", public_key.to_string(), secret_key.display_secret());

		return Ok(());
	}

	let mut swarm = create_swarm().await;

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;

	let mut dht_swarm = Dht::new(swarm);

	// let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
	let listener = TcpListener::bind("192.168.0.164:8001").await?;
	
	loop {
		tokio::select! {
			s = listener.accept() => {
				let (socket, _) = s.unwrap();
				match handler::handle_stream(socket, &mut dht_swarm).await {
					Err(err)=> {
						eprintln!("{}", err);
					}
					_ => {}
				};
			}
			// line = stdin.select_next_some() => 
			// 	handle_input(&mut dht_swarm.0.behaviour_mut(), line.expect("stdin closed")),
			event = dht_swarm.0.select_next_some() => match event {
				SwarmEvent::NewListenAddr { address, .. } => {
					println!("Listening in {:?}", address);
				},
				_ => {}
			}
		}
	}
}


// fn handle_input(behaviour: &mut MyBehaviour, line: String) {
// 	let mut args = line.split(' '); 

// 	let kad = &mut behaviour.kademlia;

// 	match args.next() {
// 		Some("GET") => {
// 			let location = {
// 				match args.next() {
// 					Some(key) => key.to_string(),
// 					None => {
// 						eprintln!("Expected location");
// 						return;
// 					}
// 				}
// 			};

// 			let mut key_idx: usize = 0;
// 			let parts: Vec<String> = location.split("/").map(|s| s.to_string()).collect();
// 			for (idx, part) in parts.iter().rev().enumerate() {
// 				if part.starts_with("e_") {
// 					key_idx = parts.len() - idx - 1;
// 					break
// 				}
// 			}

// 			let username = {
// 				match args.next() {
// 					Some(name) => name.to_string(),
// 					None => {
// 						eprintln!("Expected username");
// 						return;
// 					}
// 				}
// 			};	

// 			let query_id = kad.get_record(&Key::new(&parts[key_idx].to_string()), Quorum::One);
// 			behaviour.queries.lock().unwrap().insert(
// 				query_id, 
// 				Query { 
// 					username: String::from(username),
// 					location: parts[key_idx..].join("/")
// 				}
// 			);
// 		},
// 		Some("PUT") => {
// 			let name = {
// 				match args.next() {
// 					Some(name) => name.to_string(),
// 					None => {
// 						eprintln!("Expected name");
// 						return;
// 					}
// 				}
// 			};	

// 			let username = {
// 				match args.next() {
// 					Some(name) => name.to_string(),
// 					None => {
// 						eprintln!("Expected username");
// 						return;
// 					}
// 				}
// 			};	

// 			let public = {
// 				match args.next() {
// 					Some(value) => value == "true",
// 					None => {
// 						eprintln!("Expected true or false");
// 						return;
// 					}
// 				}
// 			};

// 			let rest: Vec<String> = args.map(|s| s.to_string()).collect();
// 			let mut _curr_idx: usize = 0;

// 			let read_users_count: usize = rest[_curr_idx as usize].parse::<usize>().unwrap() + 1;
// 			let read_users = if public {
// 				Vec::<String>::new()
// 			} else {
// 				rest[_curr_idx + 1.._curr_idx + read_users_count].to_vec()
// 			};
// 			_curr_idx += read_users_count;

// 			let children_count: usize = rest[_curr_idx as usize].parse::<usize>().unwrap() + 1;
// 			let children = rest[_curr_idx + 1.._curr_idx + children_count].to_vec();
// 			_curr_idx += children_count;

// 			let new_entry = Entry {
// 				name: name.clone(),
// 				user: username.to_string(),
// 				public,
// 				read_users: read_users,
// 				children: children.iter().filter(|s| {
// 					!s.contains("/")
// 				}).map(|s| Children {
// 					name: s.to_string(),
// 					r#type: "file".to_string(),
// 					entry: "".to_string()
// 				}).collect()
// 			};

// 			let value = serde_json::to_vec(&new_entry).unwrap();

// 			let key: String = "e_".to_string();

// 			let record = Record {
// 				key: Key::new(&key),
// 				value,
// 				publisher: None,
// 				expires: None,
// 			};

// 			kad
// 				.put_record(record, Quorum::One)
// 				.expect("Failed to store record locally.");
// 		},
// 		Some("GET_PROVIDERS") => {
// 			let key = {
// 				match args.next() {
// 					Some(key) => Key::new(&key),
// 					None => {
// 						eprintln!("Expected key");
// 						return;
// 					}
// 				}
// 			};

// 			kad.get_providers(key);
// 		},
// 		Some("PUT_PROVIDER") => {
// 			let key = {
// 				match args.next() {
// 					Some(key) => Key::new(&key),
// 					None => {
// 						eprintln!("Expected key");
// 						return;
// 					}
// 				}
// 			};

// 			kad
// 				.start_providing(key)
// 				.expect("Failed to start providing key");
// 		},
// 		_ => {}
// 	}
// }
