use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, record::Key};
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
};
use async_std::task;
use std::error::Error;
use std::{str, env};
use std::str::FromStr;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{PublicKey, Secp256k1, SecretKey, Message};
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;

use api::api_server::{Api, ApiServer};
use api::{GetRequest, GetResponse, PutResponse, PutRequest, Entry};

use tonic::{transport::Server, Request, Response, Status, Code};
use tokio::sync::{mpsc, broadcast};
use futures::stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use tokio::sync::Mutex;
use std::sync::Arc;

mod api {
	tonic::include_proto!("api");
}

mod entry;
mod behaviour;
mod handler;
mod dht;

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

#[derive(Debug)]
pub struct DhtGetRecordRequest {
	pub location: String
}

#[derive(Debug)]
pub struct DhtPutRecordRequest {
        pub public_key: PublicKey,
	pub entry: Entry,
	pub signature: String,
}

#[derive(Debug)]
pub enum DhtRequestType {
	GetRecord(DhtGetRecordRequest),
	PutRecord(DhtPutRecordRequest),
}

#[derive(Debug, Clone)]
pub struct DhtGetRecordResponse {
	pub entry: Option<Entry>,
	pub error: Option<String>
}

impl DhtGetRecordResponse {
        fn default() -> Result<Response<GetResponse>, Status> {
                Ok(Response::new(GetResponse {
                        entry: None
                }))
        }

	fn not_found() -> Result<Response<GetResponse>, Status>  {
		Err(Status::new(Code::NotFound, "Entry not found"))
	}
}

#[derive(Debug, Clone)]
pub struct DhtPutRecordResponse {
	pub signature: Option<String>,
	pub error: Option<(Code, String)>
}

#[derive(Debug, Clone)]
pub enum DhtResponseType {
	GetRecord(DhtGetRecordResponse),
	PutRecord(DhtPutRecordResponse),
}

pub struct MyApi {
	pub mpsc_sender: mpsc::Sender<DhtRequestType>,
	pub broadcast_receiver: Arc<Mutex<broadcast::Receiver<DhtResponseType>>>
}

#[tonic::async_trait]
impl Api for MyApi {
	async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
		let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
			location: request.get_ref().location.to_owned()
		});

		self.mpsc_sender.send(dht_request).await;
		match self.broadcast_receiver.lock().await.recv().await {
			Ok(dht_response) => match dht_response {
                                DhtResponseType::GetRecord(dht_get_response) => {
                                        if let Some(error) = dht_get_response.error {
						return DhtGetRecordResponse::not_found();
                                        }

                                        Ok(Response::new(GetResponse {
                                                entry: dht_get_response.entry
                                        }))
                                }
                                _ => DhtGetRecordResponse::default()
                        }
			Err(error) => {
				eprintln!("Error {}", error);
                                DhtGetRecordResponse::default()
			}
		}
	}

	async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
		let signature: String = request.get_ref().signature.clone();
		let dht_request = DhtRequestType::PutRecord(DhtPutRecordRequest {
			public_key: PublicKey::from_str(request.metadata().get("public_key").unwrap().to_str().unwrap()).unwrap(),
			signature: signature.clone(),
			entry: request.into_inner().entry.unwrap(),
		});

		self.mpsc_sender.send(dht_request).await;
		match self.broadcast_receiver.lock().await.recv().await {
                        Ok(dht_response) => match dht_response {
                                DhtResponseType::PutRecord(dht_put_response) => {
                                        if let Some((code, message)) = dht_put_response.error {
						return Err(Status::new(code, message));
                                        }

					Ok(Response::new(PutResponse {
						key: signature.clone()
					}))
                                }
                                _ => {
					println!("unknown error");
					Ok(Response::new(PutResponse {
						key: signature.clone()
					}))
				}
                        }
                        Err(error) => {
				eprintln!("{}", error);
				Ok(Response::new(PutResponse {
					key: signature
				}))
                        }
                }
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let args: Vec<String> = env::args().collect();

	if args.len() > 1 && &args[1] == "gen-keypair" {
		let secp = Secp256k1::new();
		let mut rng = OsRng::new().unwrap();
		let (secret_key, public_key) = secp.generate_keypair(&mut rng);

		println!("Public key: {}\nPrivate Key: {}", public_key.to_string(), secret_key.display_secret());

		return Ok(());
	}

	let mut swarm = create_swarm().await;
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;
	let mut dht_swarm = Dht::new(swarm);

	let (mpsc_sender, mpsc_receiver) = mpsc::channel::<DhtRequestType>(32);
	let (broadcast_sender, broadcast_receiver) = broadcast::channel::<DhtResponseType>(32);

	tokio::spawn(async move {
		let mut mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		while let Some(data) = mpsc_receiver_stream.next().await {
			match data {
				DhtRequestType::GetRecord(dht_get_record) => {
					let key = handler::get_location_key(dht_get_record.location);

					match dht_swarm.get(&key).await {
						Ok(record) => {
							let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

							broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
								entry: Some(entry),
								error: None
							})).unwrap();
                                                }
						Err(error) => {
							broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
								entry: None,
								error: Some(error.to_string())
							})).unwrap();
						}
					};
				}
				DhtRequestType::PutRecord(dht_put_record) => {
					let value = serde_json::to_vec(&dht_put_record.entry).unwrap();
					let pub_key = dht_put_record.public_key.clone();
					let key: String = format!("e_{}", dht_put_record.signature);

					let secp = Secp256k1::new();
					let sig = Signature::from_str(&dht_put_record.signature.clone()).unwrap();
					let message = Message::from_hashed_data::<sha256::Hash>(
						format!("{}/{}", pub_key, dht_put_record.entry.name).as_bytes()
					);

					match secp.verify_ecdsa(&message, &sig, &pub_key) {
						Err(error) => {
							broadcast_sender.send(DhtResponseType::PutRecord(DhtPutRecordResponse {
								signature: Some(key),
								error: Some((Code::Unauthenticated, "Invalid signature".to_string()))
							})).unwrap();
							continue;
						}
						_ => {}
					}

					let res = match dht_swarm.put(Key::new(&key.clone()), value).await {
						Ok(_) => DhtResponseType::PutRecord(DhtPutRecordResponse { 
                                                        signature: Some(key),
                                                        error: None
                                                }),
						Err(error) => DhtResponseType::PutRecord(DhtPutRecordResponse { 
                                                        // signature: None,
                                                        // error: Some((Code::Unknown, error.to_string()))
                                                        error: None,
							signature: Some(key)
                                                })
					};

                                        broadcast_sender.send(res);

				}
			};

		}
	});

	let say = MyApi {
		mpsc_sender,
		broadcast_receiver: Arc::new(Mutex::new(broadcast_receiver))
	};
	let server = Server::builder().add_service(ApiServer::new(say));

	let addr = "192.168.0.164:50051".parse().unwrap();
	println!("Server listening on {}", addr);
	server.serve(addr).await;

	Ok(())
}
