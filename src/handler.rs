use libp2p::kad::{
     record::Key,
     Quorum,
     Record,
     KademliaEvent,
     QueryResult,
     PeerRecord
};
use std::str;
use libp2p::{
    Swarm,
    swarm::SwarmEvent
};
use futures::{prelude::*};
use sha2::{Sha256, Digest};
use tokio::net::TcpStream; 
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};

use crate::entry::Entry;
use crate::behaviour::{MyBehaviour, OutEvent};

#[derive(Debug, Deserialize)]
struct GetRecordQuery {
	location: String,
	username: String
}

#[derive(Debug, Deserialize, Serialize)]
struct GetRecordResponse {
	found: bool,
	key: String,
	data: Option<Entry>
}

pub async fn handle_stream(mut stream: TcpStream, mut swarm: Swarm<MyBehaviour>) -> (Result<(), String>, Swarm<MyBehaviour>) {
	let mut buffer = [0; 1024];
	let bytes_read = match stream.read(&mut buffer).await {
		Ok(b) => b,
		Err(_error) => return (Err("Could not read bytes".to_string()), swarm)
	};

	let mut headers = [httparse::EMPTY_HEADER; 64];
	let mut req = httparse::Request::new(&mut headers);
	match req.parse(&buffer) {
		Ok(..) => {},
		Err(_error) => return (Err("Failed to parse headers".to_string()), swarm)

	};

	let request = String::from_utf8_lossy(&buffer[..bytes_read]);
	let mut parts = request.split("\r\n\r\n");
	match parts.next() {
		Some(_val) => {},
		None => return (Err("Expected body".to_string()), swarm)
	};

	let body = match parts.next() {
		Some(val) => val.to_string(),
		None => "".to_string()
	};

	if body.len() == 0 {
		return (Err("Got body of length 0".to_string()), swarm)
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

		stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: text/html\n\n{}", key).as_bytes()).await.unwrap();

		behaviour.kademlia
			.put_record(record, Quorum::One)
			.expect("Failed to store record locally.");
	} else if req.method.unwrap() == "GET" && req.path.unwrap() == "/get" {
		let query: GetRecordQuery = serde_json::from_str(&body).unwrap();
		let key = get_location_key(query.location.clone());

		behaviour.kademlia.get_record(&key, Quorum::One);

		let res = loop {
			if let SwarmEvent::Behaviour(OutEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {result, .. })) = swarm.select_next_some().await {
				break result;
			}
		};

		match res {
			QueryResult::GetRecord(Ok(ok)) => {
				for PeerRecord {
					record: Record { key, value, .. },
					..
				} in ok.records {
					let entry: Entry = serde_json::from_str(&str::from_utf8(&value).unwrap()).unwrap();

					if entry.has_access(query.username.clone()) {
						let res = GetRecordResponse {
							key: str::from_utf8(&key.to_vec()).unwrap().to_string(),
							found: true,
							data: Some(entry)
						};

						stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: application/json\n\n{}", serde_json::to_string(&res).unwrap()).as_bytes()).await.unwrap();
					} else {
						println!("Access to {:?} not allowed", key);
					}
				}
			},
			QueryResult::GetRecord(Err(_err)) => {
				let res = GetRecordResponse {
					key: str::from_utf8(&key.to_vec()).unwrap().to_string(),
					found: false,
					data: None
				};

				stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: application/json\n\n{}", serde_json::to_string(&res).unwrap()).as_bytes()).await.unwrap();
			},
			_ => {}
		};
	}

	(Ok(()), swarm)
}

fn get_location_key(input_location: String) -> Key {
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