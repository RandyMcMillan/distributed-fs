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
use std::{str, error::Error};
use std::str::FromStr;
use std::collections::HashMap;
use futures::stream::StreamExt;
use std::io::Read;
use tokio::sync::oneshot;

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
	// gRPC request receiver stream
	mpsc_receiver_stream: ReceiverStream<DhtRequestType>,
	// gRPC response sender
	broadcast_sender: broadcast::Sender<DhtResponseType>,
	// Request-response Request receiever
	requests_receiver: mpsc::Receiver<Event>
}

impl ApiHandler {
	pub fn new(
		mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
		broadcast_sender: broadcast::Sender<DhtResponseType>,
		managed_swarm: ManagedSwarm
	) -> Self {
		let  mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		let (requests_sender, requests_receiver) = mpsc::channel::<Event>(32);
		let event_loop = EventLoop::new(managed_swarm, requests_sender);

		tokio::spawn(async move {
			event_loop.run().await;
		});

		Self {
			mpsc_receiver_stream,
			broadcast_sender,
			requests_receiver
		}
	}

	pub async fn run(&mut self) {
		loop {
			tokio::select! {
				data = self.mpsc_receiver_stream.next() => {
					match data {
						Some(data) => {
							match self.handle_api_event(data).await {
								Err(error) => println!("{}", error),
								_ => {}
							};
						}
						_ => {}
					}
				}
				req = self.requests_receiver.recv() => {
					println!("Got request, {:?}", req);
					match req.unwrap() {
						Event::InboundRequest { request, channel, peer } => {
							self.handle_request_response(request, channel, peer);
						}
					}
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
		let request_id = self.managed_swarm.send_request(peer, request).await.unwrap();

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

	pub async fn handle_request_response(&mut self, req: FileRequest, channel: ResponseChannel<FileResponse>, peer: PeerId) -> Result<(), String> {
		let FileRequest(r) = req;

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

		Ok(())
	}
}

#[derive(Debug)]
pub enum Event {
	InboundRequest {
		request: FileRequest,
		channel: ResponseChannel<FileResponse>,
		peer: PeerId
	},
}

struct EventLoop {
	managed_swarm: ManagedSwarm,
	ledgers: HashMap<PeerId, u16>,
	pending_requests: HashMap<RequestId, oneshot::Sender<Result<String, Box<dyn Error + Send>>>>,
	requests_sender: mpsc::Sender<Event>,
}

impl EventLoop {
	pub fn new(
		managed_swarm: ManagedSwarm,
		requests_sender: mpsc::Sender<Event>,
	) -> Self {
		Self {
			managed_swarm,
			ledgers: Default::default(),
			pending_requests: Default::default(),
			requests_sender
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.managed_swarm.0.select_next_some() => {
					match event {
						SwarmEvent::NewListenAddr { address, .. } => {
							println!("Listening on {:?}", address);
						}
						SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
							for (peer_id, multiaddr) in list {
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
							match message {
								RequestResponseMessage::Response { response, request_id } => {
									println!("REsponse");
								}
								RequestResponseMessage::Request { request, channel, .. }  => {
									self.requests_sender.send(
										Event::InboundRequest { request, channel, peer }
									).await.unwrap();
								}
							}
						}
						SwarmEvent::Behaviour(OutEvent::Kademlia(_e)) => {}
						_ => {}
					};
				},
			}
		}
	}

	pub async fn send_request(&mut self, peer: PeerId, request: FileRequest) -> Result<FileResponse, String> {
		let (sender, receiver) = oneshot::channel();
		let request_id = self.managed_swarm.send_request(peer, request).await.unwrap();

		self.pending_requests.insert(request_id, sender);
		let res = receiver.await.unwrap();
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