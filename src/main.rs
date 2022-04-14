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

use api::api_server::{Api, ApiServer};
use api::{GetRequest, GetResponse, PutResponse, PutRequest, Entry};

use tonic::{transport::Server, Request, Response, Status};
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
pub struct DhtGetRecord {
	pub location: String
}

#[derive(Debug)]
pub struct DhtPutRecord {
	pub entry: Entry,
	pub signature: String,
	pub public_key: String
}

#[derive(Debug)]
pub enum DhtRequestType {
	GetRecord(DhtGetRecord),
	PutRecord(DhtPutRecord),
}

pub struct MyApi {
	pub mpsc_sender: mpsc::Sender<DhtRequestType>,
	pub broadcast_receiver: Arc<Mutex<broadcast::Receiver<Option<Entry>>>>
}

#[tonic::async_trait]
impl Api for MyApi {
	async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
		println!("{}", request.metadata().get("public_key").unwrap().to_str().unwrap());
		let dht_request = DhtRequestType::GetRecord(DhtGetRecord {
			location: request.get_ref().location.to_owned()
		});

		self.mpsc_sender.send(dht_request).await;

		match self.broadcast_receiver.lock().await.recv().await {
			Ok(entry) => {
				Ok(Response::new(GetResponse {
					message: format!("get {}", request.get_ref().location),
					entry
				}))
			}
			Err(error) => {
				eprintln!("{}", error);
				Ok(Response::new(GetResponse {
					message: format!("get {}", request.get_ref().location),
					entry: None
				}))
			}
		}
	}

	async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
		let dht_request = DhtRequestType::PutRecord(DhtPutRecord {
			public_key: request.metadata().get("public_key").unwrap().to_str().unwrap().to_string(),
			signature: request.get_ref().signature.clone(),
			entry: request.into_inner().entry.unwrap(),
		});

		self.mpsc_sender.send(dht_request).await;
		let dht_response = self.broadcast_receiver.lock().await.recv().await.unwrap(); 

		Ok(Response::new(PutResponse {
			key: "key".to_string()
		}))
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
	let (broadcast_sender, broadcast_receiver) = broadcast::channel::<Option<Entry>>(32);

	tokio::spawn(async move {
		let mut mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		while let Some(data) = mpsc_receiver_stream.next().await {
			match data {
				DhtRequestType::GetRecord(dht_get_record) => {
					let key = handler::get_location_key(dht_get_record.location);

					match dht_swarm.get(&key).await {
						Ok(record) => {
							let record: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();
							broadcast_sender.send(Some(record)).unwrap();
						}
						Err(error) => {
							eprintln!("{:?}", error);
							broadcast_sender.send(None).unwrap();
						}
					};
				},
				DhtRequestType::PutRecord(dht_put_record) => {
					// let secp = Secp256k1::new();
					// let secret_key = SecretKey::from_str("566f43d8c8436138c980dcf7f47a960282bd94af418d3f36cac68048a0c8a878").unwrap();
					// let pub_key = PublicKey::from_secret_key(&secp, &secret_key).to_string();

					let value = serde_json::to_vec(&dht_put_record.entry).unwrap();

					// let message = Message::from_hashed_data::<sha256::Hash>(format!("{}/{}", pub_key.to_string(), dht_put_record.entry.name).as_bytes());
					// let sig = secp.sign_ecdsa(&message, &secret_key);

					let key: String = format!("e_{}", dht_put_record.signature);

					match dht_swarm.put(Key::new(&key.clone()), value).await {
						Ok(_) => {
							// broadcast_sender.send(key.to_owned()).unwrap();
						}
						Err(error) => {
							eprintln!("{:?}", error);
							// broadcast_sender.send(key.to_owned()).unwrap();
						}
					};

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
