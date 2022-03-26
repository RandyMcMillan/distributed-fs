use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     record::Key,
     Quorum,
     Record,
     KademliaEvent
};
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
    swarm::SwarmEvent
};
use async_std::{task, io};
use futures::{prelude::*};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
// use rand::{thread_rng, Rng, distributions::Alphanumeric};
use sha2::{Sha256, Digest};
use tokio::net::TcpListener; 
use tokio::io::AsyncReadExt;
use serde::Deserialize;

mod entry;
mod behaviour;
// mod routes;

use entry::{Entry, Children};
use behaviour::{MyBehaviour, Query, OutEvent};

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
		queries: Arc::new(Mutex::new(HashMap::new()))
	};
        Swarm::new(transport, behaviour, local_peer_id)
}

#[derive(Debug, Deserialize)]
struct GetRecordQuery {
	location: String,
	username: String
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	// let rand_string: String = thread_rng()
	// 	.sample_iter(&Alphanumeric)
	// 	.take(30)
	// 	.map(char::from)
	// 	.collect();
	// println!("{}", rand_string);

	let mut swarm = create_swarm().await;

	let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

	// Listen on all interfaces and whatever port the OS assigns.
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;

	let listener = TcpListener::bind("192.168.0.164:8000").await?;
	
	loop {
		tokio::select! {
			s = listener.accept() => {
				let (mut socket, _) = s.unwrap();

				let mut buffer = [0; 1024];
				let bytes_read = match socket.read(&mut buffer).await {
					Ok(b) => b,
					Err(_error) => continue,
				};

				let mut headers = [httparse::EMPTY_HEADER; 64];
				let mut req = httparse::Request::new(&mut headers);
				match req.parse(&buffer) {
					Ok(..) => {},
					Err(error) => {
						eprintln!("{:?}", error);
						continue
					}
				};

				let request = String::from_utf8_lossy(&buffer[..bytes_read]);
				let mut parts = request.split("\r\n\r\n");
				match parts.next() {
					Some(_val) => {},
					None => {
						eprintln!("Expected body");
						continue
					}
				};

				let body = match parts.next() {
					Some(val) => val.to_string(),
					None => "".to_string()
				};

				if body.len() == 0 {
					continue
				}

				println!("{}, {}", req.method.unwrap(), req.path.unwrap());

				let behaviour = swarm.behaviour_mut();

				if req.method.unwrap() == "POST" && req.path.unwrap() == "/put" {
					let entry: Entry = serde_json::from_str(&body).unwrap();

					let value = serde_json::to_vec(&entry).unwrap();

					let mut hasher = Sha256::new();
					hasher.update(format!("{}{}", entry.user, entry.name));
					let key: String = format!("e_{:X}", hasher.finalize());

					let record = Record {
						key: Key::new(&key),
						value,
						publisher: None,
						expires: None,
					};
					println!("{:?}", entry);


					behaviour.kademlia
						.put_record(record, Quorum::One)
						.expect("Failed to store record locally.");
				} else if req.method.unwrap() == "GET" && req.path.unwrap() == "/get" {
					let query: GetRecordQuery = serde_json::from_str(&body).unwrap();
					let key = get_location_key(query.location.clone());
					println!("{:?}\n{:?}", key, query);

					let query_id = behaviour.kademlia.get_record(&key, Quorum::One);

					let res = loop {
						if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {result, .. })) = swarm.select_next_some().await {
							break result;
						}
					};

					println!("Query response: \n{:?}", res);
				}
			}
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

fn get_location_key(input_location: String) -> Key  {
	let mut key_idx: usize = 0;
	let parts: Vec<String> = input_location.split("/").map(|s| s.to_string()).collect();

	for (idx, part) in parts.iter().rev().enumerate() {
		if part.starts_with("e_") {
			key_idx = parts.len() - idx - 1;
			break
		}
	}

	Key::new(&parts[key_idx])
}

fn handle_input(behaviour: &mut MyBehaviour, line: String) {
	let mut args = line.split(' '); 

	let kad = &mut behaviour.kademlia;

	match args.next() {
		Some("GET") => {
			let location = {
				match args.next() {
					Some(key) => key.to_string(),
					None => {
						eprintln!("Expected location");
						return;
					}
				}
			};

			let mut key_idx: usize = 0;
			let parts: Vec<String> = location.split("/").map(|s| s.to_string()).collect();
			for (idx, part) in parts.iter().rev().enumerate() {
				if part.starts_with("e_") {
					key_idx = parts.len() - idx - 1;
					break
				}
			}

			let username = {
				match args.next() {
					Some(name) => name.to_string(),
					None => {
						eprintln!("Expected username");
						return;
					}
				}
			};	

			let query_id = kad.get_record(&Key::new(&parts[key_idx].to_string()), Quorum::One);
			behaviour.queries.lock().unwrap().insert(
				query_id, 
				Query { 
					username: String::from(username),
					location: parts[key_idx..].join("/")
				}
			);
		},
		Some("PUT") => {
			let name = {
				match args.next() {
					Some(name) => name.to_string(),
					None => {
						eprintln!("Expected name");
						return;
					}
				}
			};	

			let username = {
				match args.next() {
					Some(name) => name.to_string(),
					None => {
						eprintln!("Expected username");
						return;
					}
				}
			};	

			let public = {
				match args.next() {
					Some(value) => value == "true",
					None => {
						eprintln!("Expected true or false");
						return;
					}
				}
			};

			let rest: Vec<String> = args.map(|s| s.to_string()).collect();
			let mut _curr_idx: usize = 0;

			let read_users_count: usize = rest[_curr_idx as usize].parse::<usize>().unwrap() + 1;
			let read_users = if public {
				Vec::<String>::new()
			} else {
				rest[_curr_idx + 1.._curr_idx + read_users_count].to_vec()
			};
			_curr_idx += read_users_count;

			let children_count: usize = rest[_curr_idx as usize].parse::<usize>().unwrap() + 1;
			let children = rest[_curr_idx + 1.._curr_idx + children_count].to_vec();
			_curr_idx += children_count;

			let new_entry = Entry {
				name: name.clone(),
				user: username.to_string(),
				public,
				read_users: read_users,
				children: children.iter().filter(|s| {
					!s.contains("/")
				}).map(|s| Children {
					name: s.to_string(),
					r#type: "file".to_string(),
					entry: "".to_string()
				}).collect()
			};

			let value = serde_json::to_vec(&new_entry).unwrap();

			let mut hasher = Sha256::new();
			hasher.update(format!("{}{}", username, name));
			let key: String = format!("e_{:X}", hasher.finalize());

			let record = Record {
				key: Key::new(&key),
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
