use clap::{Parser, Subcommand};
use secp256k1::hashes::sha256;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{Message, Secp256k1, SecretKey, Signature};
use std::error::Error;

use gnostr_p2p::node::Node;

#[derive(Debug, Parser)]
#[command(name = "gnostr-p2p", version, about = "Distributed storage node")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, default_value = "storage", value_parser = ["api", "storage"])]
    role: String,

    #[arg(long, default_value = "127.0.0.1")]
    addr: String,
}

#[derive(Debug, Subcommand)]
enum Command {
    GenKeypair,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    if matches!(cli.command, Some(Command::GenKeypair)) {
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

    let swarm_addr = format!("/ip4/{}/tcp/0", cli.addr);
    let api_addr = format!("{}:50051", cli.addr);

    let bootstrap_nodes: Vec<libp2p::Multiaddr> = vec![
        // Example bootstrap nodes (replace with actual public nodes for a real deployment)
        // "/ip4/127.0.0.1/tcp/4001/p2p/Qm..." // Example format
    ];

    let node = {
        if cli.role == "api" {
            Node::new_api_node(&swarm_addr, &api_addr, bootstrap_nodes).await.unwrap()
        } else {
            Node::new_storage_node(&swarm_addr, bootstrap_nodes).await.unwrap()
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
