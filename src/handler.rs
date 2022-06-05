use tokio_stream::wrappers::ReceiverStream;
use libp2p::PeerId;
use libp2p::kad::record::Key;
use std::path::Path;
use std::fs;
use libp2p::request_response::ResponseChannel;
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
use tokio::sync::oneshot;

use crate::swarm::ManagedSwarm;
use crate::api;
use crate::entry::Entry;
use crate::behaviour::{
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
use crate::event_loop::{
	ReqResEvent,
	DhtEvent,
	EventLoop
};

pub struct ApiHandler {
	// gRPC request receiver stream
	mpsc_receiver_stream: ReceiverStream<DhtRequestType>,
	// gRPC response sender
	broadcast_sender: broadcast::Sender<DhtResponseType>,
	// Request-response Request receiever
	requests_receiver: mpsc::Receiver<ReqResEvent>,
	// DHT events for eventloop sender
	dht_event_sender: mpsc::Sender<DhtEvent>
}

impl ApiHandler {
	pub fn new(
		mpsc_receiver: mpsc::Receiver<DhtRequestType>, 
		broadcast_sender: broadcast::Sender<DhtResponseType>,
		managed_swarm: ManagedSwarm
	) -> Self {
		let  mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
		let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);
		let event_loop = EventLoop::new(managed_swarm, requests_sender, dht_event_receiver);

		tokio::spawn(async move {
			event_loop.run().await;
		});

		Self {
			mpsc_receiver_stream,
			broadcast_sender,
			requests_receiver,
			dht_event_sender
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
					match req.unwrap() {
						ReqResEvent::InboundRequest { request, channel, peer } => {
							match self.handle_request_response(request, channel, peer).await {
								Err(error) => eprint!("Error in handle_request_response: {}", error),
								_ => {}
							};
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


				let (sender, receiver) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::GetRecord { key: key.clone(), sender }
				).await.unwrap();
				match receiver.await.unwrap() {
					Ok(record) => {
						let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

						let (sender, receiver) = oneshot::channel();
						self.dht_event_sender.send(
							DhtEvent::GetProviders { key, sender }
						).await.unwrap();
						match receiver.await.unwrap() {
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
				let k = Key::new(&key.clone());

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

				let (sender, receiver) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::PutRecord { sender, key: k, value }
				).await.unwrap();
				let res = match receiver.await.unwrap() {
					Ok(key) => {
						let (sender, receiver) = oneshot::channel();
						self.dht_event_sender.send(
							DhtEvent::GetProviders { key: key.clone(), sender }
						).await.unwrap();
						let peers = receiver.await.unwrap().unwrap(); 
						let key = String::from_utf8(key.clone().to_vec()).unwrap();

						if !peers.is_empty() {
							let request = FileRequest(FileRequestType::ProvideRequest(key.clone()));

							let (sender, receiver) = oneshot::channel();
							self.dht_event_sender.send(
								DhtEvent::SendRequest { sender, request, peer: peers[0]  }
							).await.unwrap();

							match receiver.await.unwrap() {
								Ok(_res) => {},
								Err(error) => eprint!("Start providing err: {}", error)
							};
						}

						DhtResponseType::PutRecord(DhtPutRecordResponse { 
							signature: Some(key),
							error: None
						})
					},
					Err(error) => DhtResponseType::PutRecord(DhtPutRecordResponse { 
						signature: None,
						error: Some((Code::Unknown, error.to_owned()))
						// error: None,
						// signature: Some(key)
					})
				};

				self.broadcast_sender.send(res).unwrap();
			}
		};

		Ok(())
	}

	pub async fn handle_request_response(&mut self, req: FileRequest, channel: ResponseChannel<FileResponse>, peer: PeerId) -> Result<(), String> {
		let FileRequest(r) = req;

		match r {
			FileRequestType::ProvideRequest(key) => {
				let k = Key::from(key.as_bytes().to_vec());
				let response = FileResponse(FileResponseType::ProvideResponse("Started providing".to_owned())); 

				let (sender1, receiver1) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::SendResponse { sender: sender1, response, channel }
				).await.unwrap();
				receiver1.await.unwrap().unwrap();

				let (sender2, receiver2) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::StartProviding { key: k.clone(), sender: sender2 }
				).await.unwrap();
				match receiver2.await.unwrap() {
					Err(error) => return Err(error),
					_ => {}
				};

				let (sender3, receiver3) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::GetRecord { key: k, sender: sender3 }
				).await.unwrap();
				match receiver3.await.unwrap() {
					Ok(record) => {
						let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();
						println!("{:#?}", entry);
						
						if !entry.metadata.children.is_empty() {
							let get_cid = entry.metadata.children[0].cid.as_ref().unwrap();
							let request = FileRequest(FileRequestType::GetFileRequest(get_cid.to_owned()));

							let (sender, receiver) = oneshot::channel();
							self.dht_event_sender.send(
								DhtEvent::SendRequest { sender, request, peer }
							).await.unwrap();
							match receiver.await.unwrap() {
								Ok(response) => {
									match response.0 {
										FileResponseType::GetFileResponse(GetFileResponse { cid, content }) => {
											let p = format!("./cache/2/{}", cid);
											let path = Path::new(&p);

											match fs::write(path, content) {
												Err(error) => eprint!("error while writing file...\n {}", error),
												_ => {}
											};

										}
										_ => {}
									}
								}
								Err(error) => eprint!("Error while sending request: {}", error),
							}
						}
					}						
					Err(error) => {
						eprintln!("Error while getting record: {:?}", error);
					}
				};
			}
			FileRequestType::GetFileRequest(cid) => {
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

				let response = FileResponse(FileResponseType::GetFileResponse(GetFileResponse {
					content,
					cid
				}));

				let (sender, receiver) = oneshot::channel();
				self.dht_event_sender.send(
					DhtEvent::SendResponse { sender, response, channel }
				).await.unwrap();
				match receiver.await.unwrap() {
					Ok(_) => {},
					Err(_error) => {}
				};
			}
		}

		Ok(())
	}
}
