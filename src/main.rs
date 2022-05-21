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
mod swarm;
mod handler;

use swarm::ManagedSwarm;
use api::{
	MyApi,
	DhtRequestType,
	DhtResponseType
};

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

	if args.len() < 3 {
		println!("Provide server_addr");
		return Ok(());
	}

	let managed_swarm = ManagedSwarm::new("/ip4/192.168.0.248/tcp/0").await;

	let (mpsc_sender, mpsc_receiver) = mpsc::channel::<DhtRequestType>(32);
	let (broadcast_sender, broadcast_receiver) = broadcast::channel::<DhtResponseType>(32);

	tokio::spawn(async move { 
		let mut h = handler::ApiHandler::new(mpsc_receiver, broadcast_sender, managed_swarm);
		h.run().await;
	});

	let api = MyApi {
		mpsc_sender,
		broadcast_receiver: Arc::new(Mutex::new(broadcast_receiver))
	};
	let server = Server::builder().add_service(ServiceServer::new(api));

	let addr = args[2].parse().unwrap();
	println!("Server listening on {}", addr);
	server.serve(addr).await.unwrap();

	Ok(())
}
