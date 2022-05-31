use tokio_stream::wrappers::ReceiverStream;
use libp2p::PeerId;
use libp2p::kad::record::Key;
use libp2p::mdns::MdnsEvent;
use libp2p::swarm::SwarmEvent;
use std::path::Path;
use std::fs;
use libp2p::request_response::{
	RequestResponseEvent,
	RequestResponseMessage,
	RequestId,
	ResponseChannel
};
use tokio::sync::{mpsc, broadcast};
use secp256k1::{Secp256k1, Message};
use std::io::BufReader;
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;
use tonic::Code;
use std::{str, error::Error, io};
use std::str::FromStr;
use std::collections::HashMap;
use futures::stream::StreamExt;
use std::io::Read;
use tokio::sync::oneshot;
use libp2p_core::either::EitherError;

use crate::swarm::ManagedSwarm;
use crate::api;
use crate::entry::Entry;
use crate::behaviour::{
	OutEvent,
	FileResponse,
	FileRequest,
	FileRequestType,
	FileResponseType,
	GetFileResponse
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
	managed_swarm: ManagedSwarm,
}

impl ApiHandler {
	pub fn new(
		mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
		broadcast_sender: broadcast::Sender<DhtResponseType>,
		managed_swarm: ManagedSwarm
	) -> Self {
		let  mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		let event_loop = EventLoop::new(managed_swarm);

		Self {
			mpsc_receiver_stream,
			broadcast_sender,
			managed_swarm,
		}
	}

	pub async fn run(&mut self) {

		loop {
			tokio::select! {
				// event = self.managed_swarm.0.select_next_some() => {
				// 	match event {
				// 		SwarmEvent::NewListenAddr { address, .. } => {
				// 			println!("Listening on {:?}", address);
				// 		}
				// 		SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
				// 			for (peer_id, multiaddr) in list {
				// 				// println!("discovered {:?}", peer_id);
				// 				self.managed_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);

				// 				self.ledgers.entry(peer_id).or_insert(0);
				// 			}
				// 		}
				// 		SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
				// 			for (peer_id, multiaddr) in list {
				// 				println!("expired {:?}", peer_id);
				// 				self.managed_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
				// 					.expect("Error removing address");
				// 				self.ledgers.remove(&peer_id);
				// 			}
				// 		}
				// 		SwarmEvent::Behaviour(OutEvent::RequestResponse(
				// 			RequestResponseEvent::Message { message, peer },
				// 		)) => {
				// 			// println!("request response event:{:?}",message);
				// 			match self.handle_request_response(message, peer).await {
				// 				Err(error) => println!("{}", error),
				// 				_ => {}
				// 			}; 
				// 		}
				// 		SwarmEvent::Behaviour(OutEvent::Kademlia(_e)) => {
				// 			// println!("OTHER KAD: \n{:?}", e);
				// 		}
				// 		_ => {}
				// 	}
				// }
				data = self.mpsc_receiver_stream.next() => match data {
					Some(data) => {
						match self.handle_api_event(data).await {
							Err(error) => println!("{}", error),
							_ => {}
						};
					}
					_ => {}
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
				let loc = signature.clone();

				let (key, location, _signature) = api::get_location_key(signature.clone()).unwrap();

				let secp = Secp256k1::new();
				let sig = Signature::from_str(&name.clone()).unwrap();
				let message = Message::from_hashed_data::<sha256::Hash>(loc.clone().as_bytes());

				// let test_sig = secp.sign_ecdsa(&message, &SecretKey::from_str("4b3bee129b6f2a9418d1a617803913e3fee922643c628bc8fb48e0b189d104de").unwrap());
				// println!("MSG: {:?}\nSignature: {:?}\nLocation: {:?}\nExpected signature: {:?}", message, sig, loc, test_sig);

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

				match self.managed_swarm.get(&key).await {
					Ok(record) => {
						let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

						match self.managed_swarm.get_providers(key).await {
							Ok(peers) => {
								if peers.len() != 0  && entry.metadata.children.len() != 0 {
									let _get_cid = entry.metadata.children[0].cid.as_ref().unwrap();

									// println!("{:?}, {:?}", peers, entry.metadata);
									// println!("Got here");
									// self.managed_swarm.0.behaviour_mut()
									// 	.request_response
									// 	.send_request(&peers[0], FileRequest(FileRequestType::GetFileRequest(get_cid.to_owned())));
								}

								self.broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
									entry: Some(entry),
									error: None,
									location: Some(location)
								})).unwrap();
							}
							Err(error) => eprint!("{}", error)
						};

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

				let res = match self.managed_swarm.put(Key::new(&key.clone()), value).await {
					Ok(key) => {
						let peers = self.managed_swarm.get_providers(key.clone()).await.unwrap();
						let key = String::from_utf8(key.clone().to_vec()).unwrap();

						if !peers.is_empty() {
							match self.send_request(
								peers[0], 
								FileRequest(FileRequestType::ProvideRequest(key.clone()))
							).await {
								Ok(_res) => {},
								Err(error) => eprint!("Start providing err: {}", error)
							};
						}


						DhtResponseType::PutRecord(DhtPutRecordResponse { 
							signature: Some(key),
							error: None
						})
					},
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

	pub async fn send_request(&mut self, peer: PeerId, request: FileRequest) -> Result<FileResponse, String> {

		// let (sender, receiver) = oneshot::channel();
		let request_id = self.managed_swarm.send_request(peer, request).await.unwrap();

		// tokio::spawn(async {
		// 	let res = receiver.await.unwrap();
		// 	println!("receiver: {:?}", res);
		// });
		// Ok(res)
		Err("test".to_owned())
	}

	pub async fn send_response(&mut self, response: FileResponse, channel: ResponseChannel<FileResponse>) -> Result<(), String> {
		let behaviour=  self.managed_swarm.0.behaviour_mut();

		behaviour
			.request_response
			.send_response(channel, response)
			.unwrap();

		Ok(())
	}
}

struct EventLoop {
	managed_swarm: ManagedSwarm,
	ledgers: HashMap<PeerId, u16>,
	pending_requests: HashMap<RequestId, oneshot::Sender<Result<String, Box<dyn Error + Send>>>>,
}

impl EventLoop {
	pub fn new(
		managed_swarm: ManagedSwarm
	) -> Self {
		Self {
			managed_swarm,
			ledgers: Default::default(),
			pending_requests: Default::default()
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.managed_swarm.0.select_next_some() => {
					// self.handle_event(event).await;
					match event {
						SwarmEvent::NewListenAddr { address, .. } => {
							println!("Listening on {:?}", address);
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
							for (peer_id, multiaddr) in list {
								// println!("discovered {:?}", peer_id);
								self.managed_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);

								self.ledgers.entry(peer_id).or_insert(0);
							}
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
							for (peer_id, multiaddr) in list {
								println!("expired {:?}", peer_id);
								self.managed_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
									.expect("Error removing address");
								self.ledgers.remove(&peer_id);
							}
						}
						SwarmEvent::Behaviour(OutEvent::RequestResponse(
							RequestResponseEvent::Message { message, peer },
						)) => {
							// println!("request response event:{:?}",message);
							self.handle_request_response(message, peer);
						}
						SwarmEvent::Behaviour(OutEvent::Kademlia(_e)) => {
							// println!("OTHER KAD: \n{:?}", e);
						}
						_ => {}
					};
				},
			}
		}
	}

	// pub async fn handle_event(
	// 	&mut self,
	// 	event: SwarmEvent<
	// 		OutEvent,
	// 		EitherError<EitherError<io::Error, String>, String>
	// 	>,
	// ) -> Result<(), String> {

	// 	Ok(())
	// }

	pub async fn handle_request_response(&mut self, message: RequestResponseMessage<FileRequest, FileResponse>, peer: PeerId) -> Result<(), String> {
		match message {
			RequestResponseMessage::Request { request, channel, .. } => {
				let FileRequest(r) = request;
				match r {
					FileRequestType::ProvideRequest(key) => {
						let k = Key::from(key.as_bytes().to_vec());

						self.send_response(
							FileResponse(FileResponseType::ProvideResponse("Started providing".to_owned())), 
							channel
						).await.unwrap();

						match self.managed_swarm.start_providing(k.clone()).await {
							Err(error) => return Err(error),
							_ => {}
						};

						// self.managed_swarm.0.behaviour_mut()
						// 	.request_response
						// 	.send_response(channel, 
						// 		FileResponse(FileResponseType::ProvideResponse("Started providing".to_owned()))
						// 	)
						// 	.unwrap();

						match self.managed_swarm.get(&k).await {
							Ok(record) => {
								let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();
								println!("{:#?}", entry);
								
								if !entry.metadata.children.is_empty() {
									let get_cid = entry.metadata.children[0].cid.as_ref().unwrap();

									match self.send_request(
										peer, 
										FileRequest(FileRequestType::GetFileRequest(get_cid.to_owned()))
									).await {
										Ok(response) => {
											println!("Response {:?}", response);
										}
										Err(error) => eprint!("Error while sending request: {}", error)
									}
								}
							}						
							Err(error) => {
								eprintln!("Error while getting record: {:?}", error);
							}
						};
					}
					FileRequestType::GetFileRequest(cid) => {
						// println!("Get File Request: {:?}", cid);
						let location = format!("./cache/{}", cid.clone());

						let content = {
							if Path::new(&location).exists() {
								let f = fs::File::open(&location).unwrap();
								let mut reader = BufReader::new(f);
								let mut buffer = Vec::new();
								
								reader.read_to_end(&mut buffer).unwrap();

								buffer
							} else {
								println!("doesn't exists");
								Vec::new()
							}
						};
						


						self.send_response(
							FileResponse(FileResponseType::GetFileResponse(GetFileResponse {
								content,
								cid
							})),
							channel
						).await.unwrap();
					}
				}
			}
			RequestResponseMessage::Response { response, request_id } => {
				let FileResponse(response) = response;

				// println!("Got response: {:?}", self.pending_request);
				let s = self.pending_requests.remove(&request_id).unwrap();
				s.send(Ok("test".to_owned())).unwrap();

				match response {
					FileResponseType::GetFileResponse(GetFileResponse { content, cid }) => {
						let location = format!("./cache/2/{}", cid);
						let path: &Path = Path::new(&location);

                                                let s = self.ledgers.entry(peer).or_insert(0);
                                                *s += 1u16;
                                                // println!("{:#?}", self.ledgers);

						match fs::write(path, content) {
							Err(error) => {
								eprintln!("Error while writing file: {:?}", error);
							},
							_ => {}
						}
					},
					FileResponseType::ProvideResponse(msg) => {
						println!("Start providing response: {}", msg);
					}
				}
			}
		};

		Ok(())
	}

	pub async fn send_request(&mut self, peer: PeerId, request: FileRequest) -> Result<FileResponse, String> {
		let (sender, receiver) = oneshot::channel();
		let request_id = self.managed_swarm.send_request(peer, request).await.unwrap();

		self.pending_requests.insert(request_id, sender);
		let res = receiver.await.unwrap();
		// Ok(res)
		Err("test".to_owned())
	}

	pub async fn send_response(&mut self, response: FileResponse, channel: ResponseChannel<FileResponse>) -> Result<(), String> {
		let behaviour=  self.managed_swarm.0.behaviour_mut();

		behaviour
			.request_response
			.send_response(channel, response)
			.unwrap();

		Ok(())
	}
}