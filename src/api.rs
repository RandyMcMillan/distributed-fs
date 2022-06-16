use futures::stream::StreamExt;
use libp2p::kad::record::Key;
use secp256k1::PublicKey;
use std::collections::HashMap;
use secp256k1::hashes::sha256;
use secp256k1::{Message, Secp256k1};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::entry::{Children, Entry};
use crate::service::service_server::Service;
use crate::service::{
    get_response::DownloadResponse, put_request::UploadRequest, ApiEntry, DownloadFile, GetRequest,
    GetResponse, GetResponseMetadata, PutRequest, PutResponse,
};
use tonic::{Code, Request, Response, Status};

#[derive(Debug, Clone)]
pub struct DhtGetRecordRequest {
    pub signature: String,
    pub name: String,
    pub public_key: PublicKey,
}

#[derive(Debug, Clone)]
pub struct DhtPutRecordRequest {
    pub public_key: PublicKey,
    pub entry: ApiEntry,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub enum DhtRequestType {
    GetRecord(DhtGetRecordRequest),
    PutRecord(DhtPutRecordRequest),
}

#[derive(Debug, Clone)]
pub struct DhtGetRecordResponse {
    pub entry: Option<Entry>,
    pub error: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DhtPutRecordResponse {
    pub signature: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DhtResponseType {
    GetRecord(DhtGetRecordResponse),
    PutRecord(DhtPutRecordResponse),
}

pub struct MyApi {
    pub mpsc_sender: mpsc::Sender<DhtRequestType>,
    pub broadcast_receiver: Arc<Mutex<broadcast::Receiver<DhtResponseType>>>,
}

#[async_trait::async_trait]
#[tonic::async_trait]
impl Service for MyApi {
    async fn put(
        &self,
        request: Request<tonic::Streaming<PutRequest>>,
    ) -> Result<Response<PutResponse>, Status> {
        let format_ret = |error: Option<String>, key: String| {
            let put_response = {
                if let Some(error) = error {
                    PutResponse {
                        key,
                        failed: true,
                        error: Some(error),
                    }
                } else {
                    PutResponse {
                        key,
                        failed: false,
                        error: None,
                    }
                }
            };

            Ok(Response::new(put_response))
        };

        // Get Public_key from request metadata
        let public_key: PublicKey = {
            let pkey = match request.metadata().get("public_key") {
                Some(pkey) => pkey,
                None => return Err(Status::new(Code::Unknown, "No public_key provided")),
            };

            let pkey = match pkey.to_str() {
                Ok(pkey) => pkey,
                Err(_err) => return Err(Status::new(Code::Unknown, "Public_key must be str")),
            };

            match PublicKey::from_str(pkey) {
                Ok(pkey) => pkey,
                Err(_err) => return Err(Status::new(Code::Unknown, "Invalid public_key")),
            }
        };

        let mut stream = request.into_inner();
        let mut signature: Option<String> = None;

        let mut v: HashMap<String, Vec<u8>> = HashMap::new();

        while let Some(upload) = stream.next().await {
            let upload = upload.unwrap();

            match upload.upload_request.unwrap() {
                UploadRequest::Metadata(metadata) => {
                    signature = Some(metadata.signature.clone());

                    let dht_request = DhtRequestType::PutRecord(DhtPutRecordRequest {
                        public_key,
                        signature: signature.as_ref().unwrap().clone(),
                        entry: metadata.entry.unwrap(),
                    });

                    self.mpsc_sender.send(dht_request).await.unwrap();
                    match self.broadcast_receiver.lock().await.recv().await {
                        Ok(dht_response) => match dht_response {
                            DhtResponseType::PutRecord(dht_put_response) => {
                                if let Some(message) = dht_put_response.error {
                                    // return Err(Status::new(code, message));
                                    return format_ret(Some(message), metadata.signature.clone());
                                }
                            }
                            _ => {
                                println!("unknown error");
                                return format_ret(
                                    Some("Unknown error".to_owned()),
                                    signature.unwrap().clone(),
                                );
                            }
                        },
                        Err(error) => {
                            return format_ret(Some(error.to_string()), signature.unwrap());
                        }
                    };
                }
                UploadRequest::File(file) => {
                    if !signature.is_none() {

                        let cid = Message::from_hashed_data::<sha256::Hash>(&file.content);

                        if cid.to_string() != file.cid.clone() {
                            return format_ret(Some("Invalid Cid".to_owned()), signature.unwrap());
                        }
                
                        let location = format!("./cache/{}", cid.to_string());
                        let path: &Path = Path::new(&location);

                        match fs::write(path, &file.content) {
                            Err(_error) => return format_ret(Some("Error while writing file".to_owned()), signature.unwrap()),
                            _ => {}
                        }
                    } else {
                        return Err(Status::new(
                            Code::Unknown,
                            "No metadata received".to_owned(),
                        ));
                    }
                }
            };
        }

        for (key, val) in v.iter() {
            let location = format!("./cache/{}", key);
            let path: &Path = Path::new(&location);

            match fs::write(path, val) {
                Err(error) => return Err(Status::new(Code::Unknown, error.to_string())),
                _ => {}
            }
        }

        format_ret(None, signature.unwrap())
    }

    type GetStream = ReceiverStream<Result<GetResponse, Status>>;

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<Self::GetStream>, Status> {
        let public_key = PublicKey::from_str(
            request
                .metadata()
                .get("public_key")
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();

        let request = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
            signature: request.location.to_owned(),
            public_key,
            name: request.sig.to_owned(),
        });

        self.mpsc_sender.send(dht_request.clone()).await.unwrap();
        match self.broadcast_receiver.lock().await.recv().await {
            Ok(dht_response) => match dht_response {
                DhtResponseType::GetRecord(dht_get_response) => {
                    if let Some(message) = dht_get_response.error {
                        return Err(Status::new(Code::Unauthenticated, message));
                    }

                    let entry = dht_get_response.entry.unwrap();
                    if request.download {
                        tokio::spawn(async move {
                            download_file(dht_get_response.location.unwrap(), entry, tx).await;
                        });
                    } else {
                        let children = {
                            let location = dht_get_response.location.unwrap();
                            if location == "/" {
                                entry.metadata.api_children(None)
                            } else {
                                entry.metadata.api_children(Some(location))
                            }
                        };

                        tx.send(Ok(GetResponse {
                            download_response: Some(DownloadResponse::Metadata(
                                GetResponseMetadata {
                                    entry: None,
                                    children,
                                },
                            )),
                        }))
                        .await
                        .unwrap();
                    }
                }
                _ => {
                    eprintln!("unknown error");
                }
            },
            Err(error) => {
                eprintln!("{}", error);
            }
        };

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub fn get_location_key(input_location: String) -> Result<(Key, String, String), String> {
    let mut key_idx: usize = 0;
    let mut found = false;

    let input_location = {
        if input_location.ends_with("/") {
            input_location[..input_location.len() - 1].to_string()
        } else {
            input_location
        }
    };

    let parts: Vec<String> = input_location.split("/").map(|s| s.to_string()).collect();

    for (idx, part) in parts.iter().rev().enumerate() {
        if part.starts_with("e_") {
            key_idx = parts.len() - idx - 1;
            found = true;
            break;
        }
    }

    if !found {
        return Err("No signature key found".to_string());
    }

    let signature = &parts[key_idx].clone()[2..];

    let location = {
        if key_idx == parts.len() - 1 {
            "/".to_owned()
        } else {
            parts[(key_idx + 1)..].join("/")
        }
    };
    Ok((Key::new(&parts[key_idx]), location, signature.to_string()))
}

pub fn resolve_cid(location: String, metadata: Vec<Children>) -> Result<Vec<Children>, String> {
    let mut cids = Vec::<String>::new();

    if location == "/".to_string() {
        return Ok(metadata
            .into_iter()
            .filter(|child| child.r#type == "file".to_string())
            .collect());
    }

    if let Some(child) = metadata.iter().find(|child| child.name == location) {
        if child.r#type != "file".to_string() {
            return Err("Nested entry selected".to_string());
        }

        cids.push(child.cid.as_ref().unwrap().to_string());
    } else {
        for child in metadata.iter() {
            if child.r#type == "file".to_owned() && child.name.starts_with(&location) {
                let next_char = child.name.chars().nth(location.len()).unwrap().to_string();

                if next_char == "/".to_string() {
                    cids.push(child.cid.as_ref().unwrap().to_string());
                }
            }
        }
    }

    Ok(metadata
        .into_iter()
        .filter(|child| {
            child.r#type == "file".to_string() && cids.contains(&child.cid.as_ref().unwrap())
        })
        .collect())
}

async fn download_file(
    location: String,
    entry: Entry,
    tx: mpsc::Sender<Result<GetResponse, Status>>,
) {
    const CAP: usize = 1024 * 128;

    let download_children = resolve_cid(location, entry.metadata.children).unwrap();

    for download_item in download_children.iter() {
        let location = format!("./cache/{}", download_item.cid.as_ref().unwrap());

        if Path::new(&location).exists() {
            let file = fs::File::open(&location).unwrap();

            let mut reader = BufReader::with_capacity(CAP, file);

            loop {
                let buffer = reader.fill_buf().unwrap();
                let length = buffer.len();

                if length == 0 {
                    break;
                } else {
                    tx.send(Ok(GetResponse {
                        download_response: Some(DownloadResponse::File(DownloadFile {
                            content: buffer.to_vec(),
                            cid: download_item.cid.as_ref().unwrap().to_string(),
                            name: download_item.name.clone(),
                        })),
                    }))
                    .await
                    .unwrap();
                }

                reader.consume(length);
            }
        } else {
            eprintln!("File does not exists");
        }
    }
}
