use clap::Parser;
use libp2p::kad::RecordKey as Key;
use secp256k1::hashes::{sha256, Hash};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::sync::{mpsc, oneshot};

use gnostr_p2p::api::utils::{get_cids_with_sizes, resolve_cid, split_get_file_request};
use gnostr_p2p::behaviour::{
    FileRequest, FileRequestType, FileResponse, FileResponseType, GetFileResponse,
    ProvideResponse,
};
use gnostr_p2p::constants::{MAX_CHUNK_SIZE, MAX_DHT_STORED_CHUNKS};
use gnostr_p2p::entry::{Children, Entry, EntryMetaData};
use gnostr_p2p::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use gnostr_p2p::node::NodeType;
use gnostr_p2p::swarm::ManagedSwarm;

const DEFAULT_PUBLIC_KEY: &str = "023887a11113c0c72d1f887794490e70ad0f0f7cf81ab43de2998cbdab5b7bfd5a";
const DEFAULT_PRIVATE_KEY: &str = "4b3bee129b6f2a9418d1a617803913e3fee922643c628bc8fb48e0b189d104de";
const CACHE_DIR: &str = "./cache";
const DOWNLOAD_DIR: &str = "./download";

#[derive(Debug, Parser)]
#[command(
    name = "client",
    version,
    about = "Peer-to-peer storage client",
    long_about = "Upload a single file or an entire directory tree into the decentralized network, or download content back from peers.\n\nUse --path to share a local file or directory recursively, --download to fetch content by location and signature, and --logging to control output verbosity."
)]
struct Cli {
    #[arg(
        long,
        alias = "upload",
        value_name = "PATH",
        help = "Share a file or directory from this path",
        long_help = "Share a file or directory from this path. If PATH is a file, the client shares that single file. If PATH is a directory, the client recursively shares all files under that tree and preserves relative paths in the metadata."
    )]
    path: Option<PathBuf>,

    #[arg(
        long,
        value_names = ["LOCATION", "SIG"],
        num_args = 2,
        help = "Download an entry by its location and signature",
        long_help = "Download an entry by its location and signature. LOCATION is the path inside the entry, and SIG is the entry signature used as the record key."
    )]
    download: Option<Vec<String>>,

    #[arg(
        long,
        default_value = "off",
        default_missing_value = "off",
        num_args = 0..=1,
        value_parser = ["off", "warn", "info", "debug", "trace"],
        help = "Set the log verbosity level"
    )]
    logging: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    gnostr_p2p::init_logging(&cli.logging);

    let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
    let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);

    let mut swarm = ManagedSwarm::new("/ip4/0.0.0.0/tcp/0".parse()?, Vec::new()).await;
    swarm.bootstrap().await;

    let event_loop = EventLoop::new(swarm, requests_sender, dht_event_receiver);
    tokio::spawn(async move {
        event_loop.run().await;
    });

    let request_sender = dht_event_sender.clone();
    tokio::spawn(async move {
        serve_requests(requests_receiver, request_sender).await;
    });

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_str(DEFAULT_PRIVATE_KEY)?;
    let public_key = PublicKey::from_str(DEFAULT_PUBLIC_KEY)?;

    match (cli.path, cli.download) {
        (Some(path), None) => {
            let (location, signature) =
                upload_directory(Path::new(&path), &dht_event_sender, &secp, &secret_key, &public_key).await?;
            println!("UPLOAD_OK location={} signature={}", location, signature);
            println!(
                "Download it with: cargo run --bin client -- --download {} {} --logging info",
                location, signature
            );
            if std::env::var_os("DEMO_EXIT_AFTER_UPLOAD").is_none() {
                println!("Upload submitted; keeping peer alive for inbound chunk fetches.");
                tokio::signal::ctrl_c().await?;
            }
        }
        (None, Some(download)) => {
            let mut values = download.into_iter();
            let location = values.next().ok_or("missing location")?;
            let sig = values.next().ok_or("missing signature")?;
            download_entry(location, sig, &dht_event_sender, &public_key).await?;
        }
        (None, None) => {
            return Err("use --path <PATH> or --download <LOCATION> <SIG>".into());
        }
        (Some(_), Some(_)) => {
            return Err("--path and --download are mutually exclusive".into());
        }
    }

    Ok(())
}

async fn serve_requests(
    mut requests_receiver: mpsc::Receiver<ReqResEvent>,
    dht_event_sender: mpsc::Sender<DhtEvent>,
) {
    while let Some(req) = requests_receiver.recv().await {
        let ReqResEvent::InboundRequest { request, channel, .. } = req;
        match request.0 {
            FileRequestType::GetNodeTypeRequest => {
                let response = FileResponse(FileResponseType::GetNodeTypeResponse(NodeType::ApiNode));
                let (sender, receiver) = oneshot::channel();
                let _ = dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await;
                let _ = receiver.await;
            }
            FileRequestType::ProvideRequest(_) => {
                let response = FileResponse(FileResponseType::ProvideResponse(ProvideResponse::Success));
                let (sender, receiver) = oneshot::channel();
                let _ = dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await;
                let _ = receiver.await;
            }
            FileRequestType::GetFileRequest(cids) => {
                let response_cids = cids.clone();
                let mut content = Vec::new();

                for cid in cids {
                    let location = format!("{}/{}", CACHE_DIR, cid);
                    let bytes = if Path::new(&location).exists() {
                        fs::read(&location).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    content.push(bytes);
                }

                let response = FileResponse(FileResponseType::GetFileResponse(GetFileResponse {
                    content,
                    cids: response_cids,
                }));
                let (sender, receiver) = oneshot::channel();
                let _ = dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await;
                let _ = receiver.await;
            }
        }
    }
}

async fn upload_directory(
    path: &Path,
    dht_event_sender: &mpsc::Sender<DhtEvent>,
    secp: &Secp256k1<secp256k1::All>,
    secret_key: &SecretKey,
    public_key: &PublicKey,
) -> Result<(String, String), Box<dyn Error>> {
    fs::create_dir_all(CACHE_DIR)?;
    let meta = build_metadata(path)?;
    let signature = sign_entry(secp, secret_key, public_key, &meta.name);
    let peers = wait_for_storage_nodes(dht_event_sender).await?;

    let entry = Entry {
        signature: signature.clone(),
        owner: public_key.to_string(),
        public: true,
        providers: Vec::new(),
        read_users: Vec::new(),
        metadata: EntryMetaData {
            children: meta.children.clone(),
            name: meta.name.clone(),
        },
        storage_nodes: peers.iter().map(|peer| peer.to_string()).collect(),
    };

    let value = serde_json::to_vec(&entry)?;
    let key = format!("e_{}", signature);

    let (sender, receiver) = oneshot::channel();
    dht_event_sender
        .send(DhtEvent::PutRecord {
            key: Key::new(&key),
            value,
            sender,
        })
        .await?;
    receiver.await??;

    let cids_with_sizes = get_cids_with_sizes(entry.metadata.children.clone());
    let request = FileRequest(FileRequestType::ProvideRequest(cids_with_sizes));

    for peer in peers {
        let (sender, receiver) = oneshot::channel();
        dht_event_sender
            .send(DhtEvent::SendRequest {
                peer,
                request: request.clone(),
                sender,
            })
            .await?;
        let _ = receiver.await?;
    }

    Ok((meta.name, signature))
}

async fn download_entry(
    location: String,
    sig: String,
    dht_event_sender: &mpsc::Sender<DhtEvent>,
    public_key: &PublicKey,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(DOWNLOAD_DIR)?;
    let secp = Secp256k1::new();
    let message = Message::from_hashed_data::<sha256::Hash>(location.as_bytes());
    let signature = secp256k1::ecdsa::Signature::from_str(&sig)?;
    secp.verify_ecdsa(&message, &signature, public_key)?;

    let key = Key::new(&format!("e_{}", sig));
    let entry = fetch_entry(dht_event_sender, key).await?;
    let download_children = resolve_cid(location, entry.metadata.children.clone())?;

    for child in download_children {
        let target_path = PathBuf::from(DOWNLOAD_DIR).join(&child.name);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if let Some(data) = child.data {
            fs::write(target_path, data)?;
            continue;
        }

        let cids = get_cids_with_sizes(vec![child.clone()]);
        let batches = split_get_file_request(cids);
        let peer = entry
            .storage_nodes
            .first()
            .ok_or("no storage nodes available")?
            .parse()?;
        let mut file = fs::File::create(target_path)?;
        for batch in batches {
            let response = request_file_chunks(dht_event_sender, peer, batch).await?;
            for chunk in response.content.into_iter() {
                file.write_all(chunk.as_slice())?;
            }
        }
    }

    Ok(())
}

async fn fetch_entry(
    dht_event_sender: &mpsc::Sender<DhtEvent>,
    key: Key,
) -> Result<Entry, Box<dyn Error>> {
    let (sender, receiver) = oneshot::channel();
    dht_event_sender
        .send(DhtEvent::GetRecord { key, sender })
        .await?;

    let record = match receiver.await {
        Ok(Ok(record)) => record,
        Ok(Err(error)) => return Err(error.into()),
        Err(error) => return Err(error.to_string().into()),
    };
    let entry: Entry = serde_json::from_slice(&record.value)?;
    Ok(entry)
}

async fn request_file_chunks(
    dht_event_sender: &mpsc::Sender<DhtEvent>,
    peer: libp2p::PeerId,
    cids: Vec<String>,
) -> Result<GetFileResponse, Box<dyn Error>> {
    let (sender, receiver) = oneshot::channel();
    dht_event_sender
        .send(DhtEvent::SendRequest {
            peer,
            request: FileRequest(FileRequestType::GetFileRequest(cids)),
            sender,
        })
        .await?;

    let response = match receiver.await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => return Err(error.into()),
        Err(error) => return Err(error.to_string().into()),
    };
    match response.0 {
        FileResponseType::GetFileResponse(data) => Ok(data),
        other => Err(format!("unexpected response: {:?}", other).into()),
    }
}

async fn wait_for_storage_nodes(
    dht_event_sender: &mpsc::Sender<DhtEvent>,
) -> Result<Vec<libp2p::PeerId>, Box<dyn Error>> {
    for _ in 0..60 {
        let (sender, receiver) = oneshot::channel();
        dht_event_sender
            .send(DhtEvent::GetStorageNodes { sender })
            .await?;
        let peers = match receiver.await {
            Ok(Ok(peers)) => peers,
            Ok(Err(error)) => return Err(error.into()),
            Err(error) => return Err(error.to_string().into()),
        };
        if !peers.is_empty() {
            return Ok(peers);
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Err("no storage nodes discovered".into())
}

fn sign_entry(
    secp: &Secp256k1<secp256k1::All>,
    secret_key: &SecretKey,
    public_key: &PublicKey,
    name: &str,
) -> String {
    let message = Message::from_hashed_data::<sha256::Hash>(
        format!("{}/{}", public_key, name).as_bytes(),
    );
    secp.sign_ecdsa(&message, secret_key).to_string()
}

fn build_metadata(path: &Path) -> Result<EntryMetaData, Box<dyn Error>> {
    let name = if let Some(name) = path.file_name() {
        name.to_string_lossy().to_string()
    } else {
        path.canonicalize()?
            .file_name()
            .ok_or("path must point to a file or directory")?
            .to_string_lossy()
            .to_string()
    };
    let mut children = Vec::new();
    if path.is_file() {
        children.push(build_file_child(path, path)?);
    } else {
        collect_children(path, path, &mut children)?;
    }
    Ok(EntryMetaData { children, name })
}

fn collect_children(
    root: &Path,
    current: &Path,
    children: &mut Vec<Children>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_children(root, &entry_path, children)?;
            continue;
        }

        children.push(build_file_child(root, &entry_path)?);
    }

    Ok(())
}

fn build_file_child(root: &Path, entry_path: &Path) -> Result<Children, Box<dyn Error>> {
    let data = fs::read(entry_path)?;
    let relative = if root.is_file() {
        entry_path
            .file_name()
            .ok_or("path must point to a file or directory")?
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        entry_path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/")
    };
    let mut child = Children {
        name: relative,
        r#type: "file".to_string(),
        cids: Vec::new(),
        size: data.len() as i32,
        data: None,
    };

    if data.len() <= MAX_DHT_STORED_CHUNKS as usize {
        child.data = Some(data);
    } else {
        for chunk in data.chunks(MAX_CHUNK_SIZE as usize) {
            let cid = sha256::Hash::hash(chunk).to_string();
            child.cids.push(cid.clone());
            let cache_path = Path::new(CACHE_DIR).join(&cid);
            if !cache_path.exists() {
                fs::create_dir_all(CACHE_DIR)?;
                fs::write(cache_path, chunk)?;
            }
        }
    }

    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}", prefix, suffix))
    }

    #[test]
    fn build_metadata_inlines_small_files_and_chunks_large_files() {
        let root = unique_temp_dir("distributed_fs_client");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("small.txt"), b"hello").unwrap();
        fs::write(root.join("large.bin"), vec![7u8; (MAX_DHT_STORED_CHUNKS as usize) + 32]).unwrap();

        let meta = build_metadata(&root).unwrap();
        assert_eq!(meta.name, root.file_name().unwrap().to_string_lossy());
        assert_eq!(meta.children.len(), 2);

        let small = meta.children.iter().find(|child| child.name == "small.txt").unwrap();
        assert_eq!(small.data.as_deref(), Some(b"hello".as_slice()));

        let large = meta.children.iter().find(|child| child.name == "large.bin").unwrap();
        assert!(large.data.is_none());
        assert!(!large.cids.is_empty());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn build_metadata_accepts_single_file_paths() {
        let root = unique_temp_dir("distributed_fs_client_file");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("single.txt");
        fs::write(&file, b"file payload").unwrap();

        let meta = build_metadata(&file).unwrap();
        assert_eq!(meta.name, "single.txt");
        assert_eq!(meta.children.len(), 1);
        assert_eq!(meta.children[0].name, "single.txt");
        assert_eq!(meta.children[0].data.as_deref(), Some(b"file payload".as_slice()));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn build_metadata_recurses_directories() {
        let root = unique_temp_dir("distributed_fs_client_dir");
        fs::create_dir_all(root.join("nested/deeper")).unwrap();
        fs::write(root.join("root.txt"), b"root").unwrap();
        fs::write(root.join("nested/child.txt"), b"child").unwrap();
        fs::write(root.join("nested/deeper/grandchild.txt"), b"grandchild").unwrap();

        let meta = build_metadata(&root).unwrap();
        assert_eq!(meta.children.len(), 3);
        assert!(meta.children.iter().any(|child| child.name == "root.txt"));
        assert!(meta.children.iter().any(|child| child.name == "nested/child.txt"));
        assert!(meta.children.iter().any(|child| child.name == "nested/deeper/grandchild.txt"));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn build_metadata_accepts_dot_for_current_directory() {
        let root = unique_temp_dir("distributed_fs_client_dot");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/file.txt"), b"dot").unwrap();

        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let meta = build_metadata(Path::new(".")).unwrap();
        assert_eq!(meta.children.len(), 1);
        assert_eq!(meta.children[0].name, "nested/file.txt");

        std::env::set_current_dir(prev_dir).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn signing_is_stable_for_same_input() {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_str(DEFAULT_PRIVATE_KEY).unwrap();
        let public_key = PublicKey::from_str(DEFAULT_PUBLIC_KEY).unwrap();

        let sig_a = sign_entry(&secp, &secret_key, &public_key, "folder");
        let sig_b = sign_entry(&secp, &secret_key, &public_key, "folder");

        assert_eq!(sig_a, sig_b);
    }
}
