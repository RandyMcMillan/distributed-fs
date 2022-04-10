use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::Kademlia;
use libp2p::{
    development_transport, 
    identity,
    mdns::{Mdns, MdnsConfig},
    PeerId, 
    Swarm,
    swarm::SwarmEvent
};
use async_std::task;
use std::error::Error;
use tokio::net::TcpListener; 
use std::env;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{PublicKey, Secp256k1};

use hello::say_server::{Say, SayServer};
use hello::{SayResponse, SayRequest, GetRequest, GetResponse};

use tonic::{transport::Server, Request, Response, Status};
use tokio::sync::mpsc;
use futures::stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

mod hello {
	tonic::include_proto!("hello");
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

pub struct MySay {
	mpsc_sender: mpsc::Sender<String>,
}

#[tonic::async_trait]
impl Say for MySay {
	async fn send(&self, request: Request<SayRequest>) -> Result<Response<SayResponse>, Status> {
		println!("{:?}", request);
		Ok(Response::new(SayResponse {
			message: format!("hello {}", request.get_ref().name),
		}))
	}

	async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
		self.mpsc_sender.send(request.get_ref().location.to_owned()).await;
		println!("{:?}", request.remote_addr());
		
		Ok(Response::new(GetResponse {
			message: format!("get {}", request.get_ref().location),
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

		assert_eq!(public_key.to_string(), PublicKey::from_secret_key(&secp, &secret_key).to_string());
		println!("Public key: {}\nPrivate Key: {}", public_key.to_string(), secret_key.display_secret());

		return Ok(());
	}

	let mut swarm = create_swarm().await;
	swarm.listen_on("/ip4/192.168.0.164/tcp/0".parse()?)?;
	let mut dht_swarm = Dht::new(swarm);

	let mut listener = TcpListener::bind("192.168.0.164:8000").await?;

	let addr = "192.168.0.164:50051".parse().unwrap();

	let (mut mpsc_sender, mut mpsc_receiver) = mpsc::channel::<String>(32);

	tokio::spawn(async move {
		let mut mpsc_receiver_stream = ReceiverStream::new(mpsc_receiver);

		while let Some(data) = mpsc_receiver_stream.next().await {
			println!("Data: {}", data);
		}
	});

	let say = MySay {
		mpsc_sender
	};

        println!("Server listening on {}", addr);
	       
	let server = Server::builder().add_service(SayServer::new(say));
	
	tokio::spawn(async move {
		server.serve(addr).await;
	});

	loop {
		tokio::select! {
			s = listener.accept() => {
				let (socket, _) = s.unwrap();
				match handler::handle_stream(socket, &mut dht_swarm).await {
					Err(err)=> {
						eprintln!("{}", err);
					}
					_ => {}
				};
			}
			event = dht_swarm.0.select_next_some() => {
				match event {
					SwarmEvent::NewListenAddr { address, .. } => {
						println!("Listening in {:?}", address);
					},
					_ => {}
				};
			}
		}
	}
}
