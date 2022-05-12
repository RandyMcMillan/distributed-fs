use tokio_stream::wrappers::ReceiverStream;
use libp2p::kad::record::Key;
use libp2p::mdns::MdnsEvent;
use libp2p::swarm::SwarmEvent;
use tokio::sync::{mpsc, broadcast};
use secp256k1::{Secp256k1, Message};
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;
use tonic::Code;
use std::str;
use std::str::FromStr;
use futures::stream::StreamExt;

use crate::dht::Dht;
use crate::api;
use crate::entry::Entry;
use crate::behaviour::OutEvent;
use crate::api::{
	DhtGetRecordRequest,
	DhtResponseType, 
	DhtGetRecordResponse, 
	DhtRequestType, 
	DhtPutRecordResponse, 
	DhtPutRecordRequest
};

pub struct ApiHandler {
	mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
	broadcast_sender: broadcast::Sender<DhtResponseType>,
	dht_swarm: Dht
}

impl ApiHandler {
	pub async fn run(
		mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
		broadcast_sender: broadcast::Sender<DhtResponseType>,
		dht_swarm: Dht
	) {
		let mut handler = Self {
			mpsc_receiver,
			broadcast_sender,
			dht_swarm
		};

		let mut mpsc_receiver_stream = ReceiverStream::new(handler.mpsc_receiver);

		loop {
			tokio::select! {
				event = handler.dht_swarm.0.select_next_some() => {
					match event {
						SwarmEvent::NewListenAddr { address, .. } => {
							println!("Listening on {:?}", address);
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
							for (peer_id, multiaddr) in list {
								println!("discovered {:?}", peer_id);
								handler.dht_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
							}
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
							for (peer_id, multiaddr) in list {
								println!("expired {:?}", peer_id);
								handler.dht_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
									.expect("Error removing address");
							}
						}
						_ => {
							println!("OTHER: \n{:?}\n\n", event);
						}
					}
				}
				data = mpsc_receiver_stream.next() => {
					let data = data.unwrap();

					match data {
						DhtRequestType::GetRecord(DhtGetRecordRequest {
							signature,
							name,
							public_key
						}) => {
							let (key, location, signature) = api::get_location_key(signature.clone()).unwrap();

							let secp = Secp256k1::new();
							let sig = Signature::from_str(&signature.clone()).unwrap();
							let message = Message::from_hashed_data::<sha256::Hash>(
								format!("{}/{}", public_key.to_string(), name).as_bytes()
							);

							match secp.verify_ecdsa(&message, &sig, &public_key) {
								Err(_error) => {
									handler.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
										entry: None,
										error: Some((Code::Unauthenticated, "Invalid signature".to_string())),
										location: None
									})).unwrap();
									continue;
								}
								_ => {}
							}

							match handler.dht_swarm.get(&key).await {
								Ok(record) => {
									let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

									handler.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
										entry: Some(entry),
										error: None,
										location: Some(location)
									})).unwrap();
								}
								Err(error) => {
									handler.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
										entry: None,
										error: Some((Code::NotFound, error.to_string())),
										location: None
									})).unwrap();
								}
							};
						}
						DhtRequestType::PutRecord(DhtPutRecordRequest {
							entry,
							signature,
							public_key
						}) => {
							let pub_key = public_key.clone();
							let key: String = format!("e_{}", signature.to_string());

							let secp = Secp256k1::new();
							let sig = Signature::from_str(&signature.clone()).unwrap();
							let message = Message::from_hashed_data::<sha256::Hash>(
								format!("{}/{}", pub_key.to_string(), entry.name).as_bytes()
							);

							let entry = Entry::new(signature, public_key.to_string(), entry);
							let value = serde_json::to_vec(&entry).unwrap();

							match secp.verify_ecdsa(&message, &sig, &pub_key) {
								Err(_error) => {
									handler.broadcast_sender.send(DhtResponseType::PutRecord(DhtPutRecordResponse {
										signature: Some(key),
										error: Some((Code::Unauthenticated, "Invalid signature".to_string()))
									})).unwrap();
									continue;
								}
								_ => {}
							}

							let res = match handler.dht_swarm.put(Key::new(&key.clone()), value).await {
								Ok(_) => DhtResponseType::PutRecord(DhtPutRecordResponse { 
									signature: Some(key),
									error: None
								}),
								Err(_error) => DhtResponseType::PutRecord(DhtPutRecordResponse { 
									// signature: None,
									// error: Some((Code::Unknown, error.to_string()))
									error: None,
									signature: Some(key)
								})
							};

							handler.broadcast_sender.send(res).unwrap();
						}
					};

				}

			}
		}
	}
}