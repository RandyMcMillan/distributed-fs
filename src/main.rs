use secp256k1::hashes::sha256;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{Message, Secp256k1, SecretKey};
use std::env;
use std::error::Error;
use std::str::FromStr;

mod service {
    tonic::include_proto!("api");
}

mod api;
mod behaviour;
mod constants;
mod entry;
mod event_loop;
mod handler;
mod node;
mod swarm;

use node::Node;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && &args[1] == "gen-keypair" {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().unwrap();
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);

        println!(
            "Public key: {}\nPrivate Key: {}",
            public_key.to_string(),
            secret_key.display_secret()
        );
        println!("Secret Key: {:?}", secret_key.secret_bytes());

        generate_signature(
			"e_somelocation/folder/e_3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a/folder".as_bytes(),
			"4b3bee129b6f2a9418d1a617803913e3fee922643c628bc8fb48e0b189d104de"
		);

        return Ok(());
    }

    if args.len() < 3 {
        println!("Provide type and server_addr 'cargo r storage 1.1.1.1:0000'");
        return Ok(());
    }
    // let managed_swarm = ManagedSwarm::new().await;
    let swarm_addr = "/ip4/192.168.0.248/tcp/0";
    let api_addr = &args[2];

    let node_type = args[1].clone();
    let node = {
        if node_type == "api" {
            Node::new_api_node(swarm_addr, api_addr).await.unwrap()
        } else if node_type == "storage" {
            Node::new_storage_node(swarm_addr, api_addr).await.unwrap()
        } else {
            panic!("node_type should be 'storage' or 'api'")
        }
    };

    node.run().await;

    // tokio::spawn(async move {
    //     let mut h = handler::ApiHandler::new(api_req_receiver, api_res_sender, managed_swarm);
    //     h.run().await;
    // });

    // let api = MyApi {
    //     api_req_sender,
    //     api_res_receiver: Arc::new(Mutex::new(api_res_receiver)),
    // };
    // let server = Server::builder().add_service(ServiceServer::new(api));

    // let addr = args[2].parse().unwrap();
    // println!("Server listening on {}", addr);
    // server.serve(addr).await.unwrap();

    Ok(())
}

fn generate_signature(msg: &[u8], secret_key: &str) {
    let secret_key = SecretKey::from_str(secret_key).unwrap();
    let secp = Secp256k1::new();
    let message = Message::from_hashed_data::<sha256::Hash>(msg);
    let sig = secp.sign_ecdsa(&message, &secret_key);

    println!("Signature: {}", sig);
}
