use tokio_stream::wrappers::ReceiverStream;
use libp2p::PeerId;
use libp2p::kad::record::Key;
use libp2p::mdns::MdnsEvent;
use libp2p::swarm::SwarmEvent;
use std::path::Path;
use std::fs;
use libp2p::request_response::{
	RequestResponseEvent,
	RequestResponseMessage
};
use tokio::sync::{mpsc, broadcast};
use secp256k1::{Secp256k1, Message};
use std::io::BufReader;
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;
use tonic::Code;
use std::str;
use std::str::FromStr;
use futures::stream::StreamExt;
use std::io::Read;


use crate::dht::Dht;
use crate::api;
use crate::entry::Entry;
use crate::behaviour::{
	OutEvent,
	FileResponse,
	FileRequest,
	FileRequestType,
	FileResponseType
};
use crate::api::{
	DhtGetRecordRequest,
	DhtResponseType, 
	DhtGetRecordResponse, 
	DhtRequestType, 
	DhtPutRecordResponse, 
	DhtPutRecordRequest
};

pub struct ApiHandler {
	mpsc_receiver_stream: ReceiverStream<DhtRequestType>,
	broadcast_sender: broadcast::Sender<DhtResponseType>,
	dht_swarm: Dht
}

impl ApiHandler {
	pub fn new(
		mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
		broadcast_sender: broadcast::Sender<DhtResponseType>,
		dht_swarm: Dht
	) -> Self {
		let  mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		Self {
			mpsc_receiver_stream,
			broadcast_sender,
			dht_swarm
		}
	}

	pub async fn run(&mut self) {
		loop {
			tokio::select! {
				event = self.dht_swarm.0.select_next_some() => {
					match event {
						SwarmEvent::NewListenAddr { address, .. } => {
							println!("Listening on {:?}", address);
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
							for (peer_id, multiaddr) in list {
								println!("discovered {:?}", peer_id);
								self.dht_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
							}
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
							for (peer_id, multiaddr) in list {
								println!("expired {:?}", peer_id);
								self.dht_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
									.expect("Error removing address");
							}
						}
						SwarmEvent::Behaviour(OutEvent::RequestResponse(
							RequestResponseEvent::Message { message, peer },
						)) => {
							match self.handle_request_response(message, peer).await {
								Err(error) => println!("{}", error),
								_ => {}
							}; 
						}
						SwarmEvent::Behaviour(OutEvent::Kademlia(_e)) => {
							// println!("OTHER KAD: \n{:?}", e);
						}
						_ => {}
					}
				}
				data = self.mpsc_receiver_stream.next() => {
					match self.handle_api_event(data.unwrap()).await {
						Err(error) => println!("{}", error),
						_ => {}
					};
				}
			}
		}
	}

	pub async fn handle_api_event(&mut self, data: DhtRequestType) -> Result<(), String> {
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
						self.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
							entry: None,
							error: Some((Code::Unauthenticated, "Invalid signature".to_string())),
							location: None
						})).unwrap();
						return Ok(());
					}
					_ => {}
				}

				match self.dht_swarm.get(&key).await {
					Ok(record) => {
						let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

						self.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
							entry: Some(entry),
							error: None,
							location: Some(location)
						})).unwrap();
					}
					Err(error) => {
						self.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
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
						self.broadcast_sender.send(DhtResponseType::PutRecord(DhtPutRecordResponse {
							signature: Some(key),
							error: Some((Code::Unauthenticated, "Invalid signature".to_string()))
						})).unwrap();
						return Ok(());
					}
					_ => {}
				}

				let res = match self.dht_swarm.put(Key::new(&key.clone()), value).await {
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

				self.broadcast_sender.send(res).unwrap();
			}
		};

		Ok(())
	}

	pub async fn handle_request_response(&mut self, message: RequestResponseMessage<FileRequest, FileResponse>, peer: PeerId) -> Result<(), String> {
		match message {
			RequestResponseMessage::Request { request, channel, .. } => {
				let FileRequest(r) = request;
				match r {
					FileRequestType::ProvideRequest(key) => {
						self.dht_swarm.0.behaviour_mut()
							.request_response
							.send_response(channel, 
								FileResponse(FileResponseType::ProvideResponse("Started providing".to_owned()))
							)
							.expect("Faild to send response");

						let k = Key::from(key.as_bytes().to_vec());

						match self.dht_swarm.start_providing(k.clone()).await {
							Err(error) => return Err(error),
							_ => {}
						};

						match self.dht_swarm.get(&k).await {
							Ok(record) => {
								let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();
								println!("{:#?}", entry);
								
								if entry.metadata.children.len() != 0 {
									let get_cid = entry.metadata.children[0].cid.as_ref().unwrap();

									self.dht_swarm.0.behaviour_mut()
										.request_response
										.send_request(&peer, FileRequest(FileRequestType::GetFileRequest(get_cid.to_owned())));
								}
							}						
							Err(error) => {
								eprintln!("Error while getting record: {:?}", error);
							}
						};
					}
					FileRequestType::GetFileRequest(cid) => {
						let location = format!("./cache/{}", cid);

						let contents = {
							if Path::new(&location).exists() {
								let f = fs::File::open(&location).unwrap();
								let mut reader = BufReader::new(f);
								let mut buffer = Vec::new();
								
								reader.read_to_end(&mut buffer).unwrap();

								buffer
							} else {
								Vec::new()
							}
						};

						self.dht_swarm.0.behaviour_mut()
							.request_response
							.send_response(channel, 
								FileResponse(FileResponseType::GetFileResponse(contents))
							)
							.expect("Faild to send response");
					}
				}
			}
			RequestResponseMessage::Response { response, .. } => {
				let FileResponse(response) = response;

				match response {
					FileResponseType::GetFileResponse(_content) => {

					},
					FileResponseType::ProvideResponse(msg) => {
						println!("Start providing response: {}", msg);
					}
				}
			}
		};

		Ok(())
	}
}