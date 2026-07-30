use clap::{Parser, Subcommand};
use secp256k1::hashes::sha256;
use secp256k1::rand::rngs::OsRng;
use secp256k1::{Message, Secp256k1, SecretKey, Signature};
use std::env;
use std::error::Error;

use gnostr_p2p::node::Node;

#[derive(Debug, Parser)]
#[command(
    name = "gnostr-p2p",
    version,
    about = "Distributed storage node",
    long_about = "Start an API node, storage node, or generate a signing keypair for the decentralized Rust network.\n\nUse --logging to control the verbosity of the node logs."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        long,
        default_value = "storage",
        value_parser = ["api", "storage"],
        help = "Select which node role to run"
    )]
    role: String,

    #[arg(
        long,
        default_value = "127.0.0.1",
        help = "Bind the node and gRPC listener to this host or IP"
    )]
    addr: String,

    #[arg(
        long,
        default_value = "info",
        value_parser = ["warn", "info", "debug", "trace"],
        help = "Set the log verbosity level"
    )]
    logging: String,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Generate a secp256k1 keypair for signing and identity")]
    GenKeypair,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let raw_args: Vec<String> = env::args().collect();
    if raw_args.iter().any(|arg| arg == "-h" || arg == "--help")
        && raw_args.iter().all(|arg| arg != "gen-keypair")
    {
        let role = raw_args
            .windows(2)
            .find(|pair| pair[0] == "--role")
            .map(|pair| pair[1].as_str())
            .unwrap_or("storage");
        print_role_help(role);
        return Ok(());
    }

    let cli = Cli::parse();
    gnostr_p2p::init_logging(&cli.logging);

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

    fn print_role_help(role: &str) {
        match role {
            "api" => {
                println!("Usage: gnostr-p2p --role api [--addr ADDR]\n");
                println!("API node:");
                println!("  Runs the peer that accepts client uploads/downloads and orchestrates storage peers.");
                println!("  It handles metadata writes, storage-node discovery, and request/response forwarding.");
                println!();
                println!("Options:");
                println!("  --role api     Start the API node role");
                println!("  --addr ADDR    Bind host/IP for the swarm listener and gRPC server");
                println!("  --logging LVL  Set log verbosity: warn, info, debug, or trace");
                println!();
                println!("Typical flow:");
                println!("  1. Start 1 API node and multiple storage nodes");
                println!("  2. Upload content with the Rust client binary");
                println!("  3. Download by signature/location using the Rust client binary");
            }
            _ => {
                println!("Usage: gnostr-p2p --role storage [--addr ADDR]\n");
                println!("Storage node:");
                println!("  Runs the peer that stores chunks and serves them back to requesters.");
                println!("  Use this to replicate data, answer chunk requests, and participate in DHT lookups.");
                println!();
                println!("Options:");
                println!("  --role storage Start the storage node role");
                println!("  --addr ADDR    Bind host/IP for the swarm listener");
                println!("  --logging LVL  Set log verbosity: warn, info, debug, or trace");
                println!();
                println!("Typical flow:");
                println!("  1. Start at least one storage node before uploading");
                println!("  2. Keep several storage nodes running for better replication");
                println!("  3. Use the Rust client binary to request or fetch content");
            }
        }
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
