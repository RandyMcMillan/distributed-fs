pub mod utils;

use futures::stream::StreamExt;
use secp256k1::ecdsa::Signature;
use secp256k1::hashes::sha256;
use secp256k1::{Message, PublicKey, Secp256k1};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Code, Request, Response, Status};

use crate::entry::Entry;
use crate::service::service_server::Service;
use crate::service::{
    get_response::DownloadResponse, put_request::UploadRequest, ApiEntry, GetRequest, GetResponse,
    GetResponseMetadata, PutRequest, PutRequestMetadata, PutResponse,
};
use utils::{download_file, get_cids_with_sizes, resolve_cid, split_get_file_request};

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
    pub api_req_sender: mpsc::Sender<DhtRequestType>,
    pub api_res_receiver: Arc<Mutex<broadcast::Receiver<DhtResponseType>>>,
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
                        success: false,
                        error: Some(error),
                    }
                } else {
                    PutResponse {
                        key,
                        success: true,
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
        let mut metadata: Option<PutRequestMetadata> = None;

        while let Some(upload) = stream.next().await {
            let upload = upload.unwrap();

            match upload.upload_request.unwrap() {
                UploadRequest::Metadata(data) => {
                    let secp = Secp256k1::new();
                    let sig = match Signature::from_str(&data.signature) {
                        Err(_error) => {
                            return format_ret(
                                Some("Error while parsing signature".to_owned()),
                                data.signature.clone(),
                            )
                        }
                        Ok(sig) => sig,
                    };

                    let message = Message::from_hashed_data::<sha256::Hash>(
                        format!(
                            "{}/{}",
                            public_key.to_string(),
                            data.clone().entry.unwrap().name
                        )
                        .as_bytes(),
                    );

                    match secp.verify_ecdsa(&message, &sig, &public_key) {
                        Err(_error) => {
                            return format_ret(
                                Some("Invalid signature".to_owned()),
                                data.signature.clone(),
                            )
                        }
                        _ => {}
                    }

                    metadata = Some(data);
                }
                UploadRequest::File(file) => {
                    if !metadata.is_none() {
                        let cid = Message::from_hashed_data::<sha256::Hash>(&file.content);

                        if cid.to_string() != file.cid.clone() {
                            return format_ret(
                                Some("Invalid Cid".to_owned()),
                                metadata.unwrap().signature.clone(),
                            );
                        }

                        let location = format!("./cache/{}", cid.to_string());
                        let path: &Path = Path::new(&location);

                        if path.exists() {
                            continue;
                        }

                        match fs::write(path, &file.content) {
                            Err(_error) => {
                                return format_ret(
                                    Some("Error while writing file".to_owned()),
                                    metadata.unwrap().signature.clone(),
                                )
                            }
                            _ => {}
                        };
                    } else {
                        return Err(Status::new(
                            Code::Unknown,
                            "No metadata received".to_owned(),
                        ));
                    }
                }
            };
        }

        if let Some(data) = metadata.clone() {
            let dht_request = DhtRequestType::PutRecord(DhtPutRecordRequest {
                public_key,
                signature: data.signature.clone(),
                entry: data.entry.unwrap(),
            });

            self.api_req_sender.send(dht_request).await.unwrap();
            let dht_response = match self.api_res_receiver.lock().await.recv().await {
                Ok(dht_response) => dht_response,
                Err(error) => {
                    return format_ret(Some(error.to_string()), data.signature.clone());
                }
            };

            match dht_response {
                DhtResponseType::PutRecord(dht_put_response) => {
                    if let Some(message) = dht_put_response.error {
                        return format_ret(Some(message), data.signature.clone());
                    }
                }
                _ => {
                    return format_ret(Some("Unknown error".to_owned()), data.signature.clone());
                }
            };
        }

        format_ret(None, metadata.unwrap().signature)
    }

    type GetStream = ReceiverStream<Result<GetResponse, Status>>;

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<Self::GetStream>, Status> {
        let format_err_ret = |error: String| {
            Ok(GetResponse {
                download_response: Some(DownloadResponse::Error(error)),
            })
        };
        let (res_sender, res_receiver) = mpsc::channel(4);
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

        let request = request.into_inner();

        let secp = Secp256k1::new();
        let sig = Signature::from_str(&request.sig.clone()).unwrap();
        let message =
            Message::from_hashed_data::<sha256::Hash>(request.location.clone().as_bytes());

        match secp.verify_ecdsa(&message, &sig, &public_key) {
            Err(_error) => {
                res_sender
                    .send(format_err_ret("Invalid signature".to_owned()))
                    .await
                    .unwrap();

                return Ok(Response::new(ReceiverStream::new(res_receiver)));
            }
            _ => {}
        };

        let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
            signature: request.location.to_owned(),
            public_key,
            name: request.sig.to_owned(),
        });

        self.api_req_sender.send(dht_request.clone()).await.unwrap();
        match self.api_res_receiver.lock().await.recv().await {
            Ok(dht_response) => match dht_response {
                DhtResponseType::GetRecord(dht_get_response) => {
                    if let Some(message) = dht_get_response.error {
                        res_sender.send(format_err_ret(message)).await.unwrap();

                        return Ok(Response::new(ReceiverStream::new(res_receiver)));
                    }

                    let (has_access, entry) = user_has_access(dht_get_response.entry.unwrap(), public_key);
                    if !has_access {
                        res_sender
                            .send(format_err_ret("No access allowed".to_owned()))
                            .await
                            .unwrap();

                        return Ok(Response::new(ReceiverStream::new(res_receiver)));
                    }

                    if request.download {
                        tokio::spawn(async move {
                            let location = dht_get_response.location.unwrap();
                            download_data(location.clone(), &entry).await;
                            download_file(location, entry, res_sender).await;
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

                        res_sender
                            .send(Ok(GetResponse {
                                download_response: Some(DownloadResponse::Metadata(
                                    GetResponseMetadata {
                                        entry: None,
                                        children,
                                        error: None,
                                        success: true,
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

        Ok(Response::new(ReceiverStream::new(res_receiver)))
    }
}

async fn download_data(
    location: String,
    entry: &Entry,
) {
    let download_children = resolve_cid(location.clone(), entry.metadata.children.clone()).unwrap();
    let mut download_cids_with_sizes = get_cids_with_sizes(download_children);
    download_cids_with_sizes = download_cids_with_sizes
        .into_iter()
        .filter(|(cid, _)| {
            let location = format!("./cache/{}", cid.clone());
            !Path::new(&location).exists()
        })
        .collect();

    for req_cids in split_get_file_request(download_cids_with_sizes) {
        println!("Req: {:?}", req_cids);
    };
}

fn user_has_access(
    entry: Entry,
    public_key: PublicKey
) -> (bool, Entry) {
    if entry.public {
        return (true, entry)
    }

    (entry.read_users.contains(&public_key.to_string()), entry)
}