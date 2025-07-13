use secp256k1::hashes::sha256;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{Message, Secp256k1, SecretKey, Signature};
use std::env;
use std::error::Error;

mod service {
    tonic::include_proto!("api");
}

mod api;
mod behaviour;
mod constants;
mod entry;
mod event_loop;
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
            public_key,
            secret_key.display_secret()
        );
        println!("Secret Key: {:?}", secret_key.secret_bytes());

        return Ok(());
    }

    if args.len() < 3 {
        //println!("Provide type and server_addr 'tcp_chat [api | storage] 127.0.0.1'");
        //return Ok(());
    }

    let node_type: String;
    let mut addr: &str = "127.0.0.1";
    if args.len() <= 3 {
        node_type = "storage".to_string();
        addr = "127.0.0.1";
    } else {
        node_type = args[1].clone();
        addr = &args[2];
    }

    let swarm_addr = format!("/ip4/{}/tcp/0", addr);
    let api_addr = format!("{}:50051", addr);

    let node = {
        if node_type == "api" {
            Node::new_api_node(&swarm_addr, &api_addr).await.unwrap()
        } else if node_type == "storage" {
            Node::new_storage_node(&swarm_addr).await.unwrap()
        } else {
            panic!("node_type should be 'storage' or 'api'")
        }
    };

    node.run().await;

    Ok(())
}

pub fn generate_signature(msg: &[u8], secret_key: &SecretKey) -> Signature {
    let secp = Secp256k1::new();
    let message = Message::from_hashed_data::<sha256::Hash>(msg);
    let sig = secp.sign_ecdsa(&message, secret_key);

    println!("Signature: {}", sig);
    sig
}

#[cfg(test)]
mod tests {
    use super::*;

    use secp256k1::hashes::sha256;
    use secp256k1::rand::rngs::OsRng;
    use secp256k1::{Message, Secp256k1};

    #[test]
    fn test_signatures() {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().unwrap();
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);

        // println!("Secret key: {:?}", secret_key);
        // println!("Public key: {}", public_key);
        let input = b"Some Message";
        let signature = generate_signature(input, &secret_key);

        let message = Message::from_hashed_data::<sha256::Hash>(input);
        let result = secp.verify_ecdsa(&message, &signature, &public_key);

        assert_eq!(result, Ok(()))
    }
}
