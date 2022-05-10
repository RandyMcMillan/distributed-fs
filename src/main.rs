use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::Kademlia;
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
};
use async_std::task;
use std::error::Error;
use std::env;
use secp256k1::rand::rngs::OsRng;
use secp256k1::Secp256k1;
use tonic::transport::Server;
use tokio::sync::{mpsc, broadcast};
use tokio::sync::Mutex;
use std::sync::Arc;
use service::service_server::ServiceServer;

mod service {
	tonic::include_proto!("api");
}

mod api;
mod entry;
mod behaviour;
mod dht;
mod handler;

use behaviour::MyBehaviour;
use dht::Dht;
use api::{
	MyApi,
	DhtRequestType,
	DhtResponseType
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
		request_response: MyBehaviour::create_req_res()
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
	let dht_swarm = Dht::new(swarm);

	let (mpsc_sender, mpsc_receiver) = mpsc::channel::<DhtRequestType>(32);
	let (broadcast_sender, broadcast_receiver) = broadcast::channel::<DhtResponseType>(32);

	tokio::spawn(async move { 
		handler::ApiHandler::run(mpsc_receiver, broadcast_sender, dht_swarm).await;
	});

	let api = MyApi {
		mpsc_sender,
		broadcast_receiver: Arc::new(Mutex::new(broadcast_receiver))
	};
	let server = Server::builder().add_service(ServiceServer::new(api));

	let addr = "192.168.0.164:50051".parse().unwrap();
	println!("Server listening on {}", addr);
	server.serve(addr).await.unwrap();

	Ok(())
}
