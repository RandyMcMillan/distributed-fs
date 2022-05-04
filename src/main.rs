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
use secp256k1::{Secp256k1, Message, SecretKey};
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;
use tonic::{transport::Server, Code};
use tokio::sync::{mpsc, broadcast};
use futures::stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::Mutex;
use std::sync::Arc;
use entry::Entry;

use service::service_server::{Service, ServiceServer};
use service::ApiEntry;

mod service {
	tonic::include_proto!("api");
}

mod api;
mod entry;
mod behaviour;
mod dht;

use behaviour::MyBehaviour;
use dht::Dht;
use api::{
	MyApi,
	DhtGetRecordRequest,
	DhtResponseType, 
	DhtGetRecordResponse, 
	DhtRequestType, 
	DhtPutRecordResponse, 
	DhtPutRecordRequest
};


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
						Err(error) => {
							broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
								entry: None,
								error: Some((Code::Unauthenticated, "Invalid signature".to_string())),
								location: None
							})).unwrap();
							continue;
						}
						_ => {}
					}

					match dht_swarm.get(&key).await {
						Ok(record) => {
							let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

							broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
								entry: Some(entry),
								error: None,
								location: Some(location)
							})).unwrap();
                                                }
						Err(error) => {
							broadcast_sender.send(DhtResponseType::GetRecord(DhtGetRecordResponse {
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
						Err(error) => {
							println!("{:?}", error);
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
	let server = Server::builder().add_service(ServiceServer::new(say));

	let addr = "192.168.0.164:50051".parse().unwrap();
	println!("Server listening on {}", addr);
	server.serve(addr).await;

	Ok(())
}
