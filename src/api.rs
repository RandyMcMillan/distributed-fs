use libp2p::kad::record::Key;
use secp256k1::{PublicKey, Secp256k1, SecretKey, Message};
use secp256k1::hashes::sha256;
use secp256k1::ecdsa::Signature;
use std::str::FromStr;
use futures::stream::StreamExt;
use std::fs;
use std::path::Path;
use tokio::sync::{mpsc, broadcast};
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::io::{self, BufRead, BufReader};

use tonic::{Request, Response, Status, Code};
use crate::service::service_server::Service;
use crate::service::{
	GetRequest, 
	GetResponse, 
	PutResponse, 
	PutRequest,
	ApiEntry, 
	FileUploadRequest, 
	FileUploadResponse, 
	file_upload_request::UploadRequest,
	file_download_response::DownloadResponse,
	FileDownloadResponse,
	File
};
use crate::entry::Entry;

pub fn get_location_key(input_location: String) -> Key {
	let mut key_idx: usize = 0;
	let parts: Vec<String> = input_location.split("/").map(|s| s.to_string()).collect();

	for (idx, part) in parts.iter().rev().enumerate() {
		if part.starts_with("e_") {
			key_idx = parts.len() - idx - 1;
			break
		}
	}

	Key::new(&parts[key_idx])
}

#[derive(Debug)]
pub struct DhtGetRecordRequest {
	pub signature: String,
	pub name: String,
	pub public_key: PublicKey
}

#[derive(Debug)]
pub struct DhtPutRecordRequest {
        pub public_key: PublicKey,
	pub entry: ApiEntry,
	pub signature: String,
}

#[derive(Debug)]
pub enum DhtRequestType {
	GetRecord(DhtGetRecordRequest),
	PutRecord(DhtPutRecordRequest),
}

#[derive(Debug, Clone)]
pub struct DhtGetRecordResponse {
	pub entry: Option<Entry>,
	pub error: Option<(Code, String)>
}

impl DhtGetRecordResponse {
        fn default() -> Result<Response<GetResponse>, Status> {
                Ok(Response::new(GetResponse {
                        entry: None
                }))
        }

	fn not_found() -> Result<Response<GetResponse>, Status>  {
		Err(Status::new(Code::NotFound, "Entry not found"))
	}
}

#[derive(Debug, Clone)]
pub struct DhtPutRecordResponse {
	pub signature: Option<String>,
	pub error: Option<(Code, String)>
}

#[derive(Debug, Clone)]
pub enum DhtResponseType {
	GetRecord(DhtGetRecordResponse),
	PutRecord(DhtPutRecordResponse),
}

pub struct MyApi {
	pub mpsc_sender: mpsc::Sender<DhtRequestType>,
	pub broadcast_receiver: Arc<Mutex<broadcast::Receiver<DhtResponseType>>>
}


#[tonic::async_trait]
impl Service for MyApi {
	async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
		// let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
		// 	location: request.get_ref().location.to_owned()
		// });

		// self.mpsc_sender.send(dht_request).await;
		// match self.broadcast_receiver.lock().await.recv().await {
		// 	Ok(dht_response) => match dht_response {
                //                 DhtResponseType::GetRecord(dht_get_response) => {
                //                         if let Some(error) = dht_get_response.error {
		// 				return DhtGetRecordResponse::not_found();
                //                         }

                //                         Ok(Response::new(GetResponse {
                //                                 entry: dht_get_response.entry
                //                         }))
                //                 }
                //                 _ => DhtGetRecordResponse::default()
                //         }
		// 	Err(error) => {
		// 		eprintln!("Error {}", error);
                //                 DhtGetRecordResponse::default()
		// 	}
		// }
		DhtGetRecordResponse::default()
	}

	async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
		// let signature: String = request.get_ref().signature.clone();
		// let dht_request = DhtRequestType::PutRecord(DhtPutRecordRequest {
		// 	public_key: PublicKey::from_str(request.metadata().get("public_key").unwrap().to_str().unwrap()).unwrap(),
		// 	signature: signature.clone(),
		// 	entry: request.into_inner().entry.unwrap(),
		// });

		// self.mpsc_sender.send(dht_request).await;
		// match self.broadcast_receiver.lock().await.recv().await {
                //         Ok(dht_response) => match dht_response {
                //                 DhtResponseType::PutRecord(dht_put_response) => {
                //                         if let Some((code, message)) = dht_put_response.error {
		// 				return Err(Status::new(code, message));
                //                         }

		// 			Ok(Response::new(PutResponse {
		// 				key: signature.clone()
		// 			}))
                //                 }
                //                 _ => {
		// 			println!("unknown error");
		// 			Ok(Response::new(PutResponse {
		// 				key: signature.clone()
		// 			}))
		// 		}
                //         }
                //         Err(error) => {
		// 		eprintln!("{}", error);
		// 		Ok(Response::new(PutResponse {
		// 			key: signature
		// 		}))
                //         }
                // }
		Err(Status::new(Code::Unknown, "_".to_owned()))
	}

        async fn upload(
                &self, 
                request: Request<tonic::Streaming<FileUploadRequest>>
        ) -> Result<Response<FileUploadResponse>, Status> {
		let public_key = PublicKey::from_str(request.metadata().get("public_key").unwrap().to_str().unwrap()).unwrap();
                let mut stream = request.into_inner();

		let mut signature: Option<String> = None;

                let mut v: Vec<u8> = Vec::new();
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

					self.mpsc_sender.send(dht_request).await;
					match self.broadcast_receiver.lock().await.recv().await {
						Ok(dht_response) => match dht_response {
							DhtResponseType::PutRecord(dht_put_response) => {
								if let Some((code, message)) = dht_put_response.error {
									return Err(Status::new(code, message));
								}

								// return Ok(Response::new(FileUploadResponse {
								// 	key: signature.clone()
								// }));
							}
							_ => {
								println!("unknown error");
								return Ok(Response::new(FileUploadResponse {
									key: signature.unwrap().clone()
									// key: "test".to_string()
								}));
							}
						}
						Err(error) => {
							eprintln!("{}", error);
							return Ok(Response::new(FileUploadResponse {
								key: signature.unwrap()
								// key: "test".to_string()
							}));
						}
					};
					
                                }
                                UploadRequest::File(file) => {
					if !signature.is_none() {
						v.extend_from_slice(&file.content); 
						println!("{:?}", file.cid);
					} else {
						return Err(Status::new(Code::Unknown, "No metadata received".to_owned()));
					}
                                }
                        };
                }

		let location = format!("./cache/{}", signature.as_ref().unwrap().clone());
		let path: &Path = Path::new(&location);
		match fs::write(path, v) {
			Ok(_) => {
				Ok(Response::new(FileUploadResponse {
					key: format!("e_{}", signature.unwrap())
				}))
			}
			Err(error) => {
				Err(Status::new(Code::Unknown, error.to_string()))
			}
		}
        }

	type DownloadStream = ReceiverStream<Result<FileDownloadResponse, Status>>;

	async fn download(
		&self,
		request: Request<GetRequest>
	) -> Result<Response<Self::DownloadStream>, Status> {
		let public_key = PublicKey::from_str(request.metadata().get("public_key").unwrap().to_str().unwrap()).unwrap();
		let request = request.into_inner();
		let (tx, rx) = mpsc::channel(4);

		let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
			signature: request.location.to_owned(),
			public_key,
			name: request.name.to_owned()
		});
		
		self.mpsc_sender.send(dht_request).await;

		match self.broadcast_receiver.lock().await.recv().await {
			Ok(dht_response) => match dht_response {
				DhtResponseType::GetRecord(dht_get_response) => {
					println!("GOT HERE, \n{:?}", dht_get_response);
					if let Some((code, message)) = dht_get_response.error {
						return Err(Status::new(code, message));
					}

					let entry = dht_get_response.entry.unwrap();

					if request.download {
						tokio::spawn(async move {
							const CAP: usize = 1024 * 128;
							let location = format!("./cache/{}", entry.signature);

							if Path::new(&location).exists() {
								let file = fs::File::open(&location).unwrap();

								let mut reader = BufReader::with_capacity(CAP, file);

								if request.download {
									loop {
										let buffer = reader.fill_buf().unwrap();
										let length = buffer.len();

										if length == 0 {
											break
										} else {
											tx.send(Ok(FileDownloadResponse {
												download_response: Some(DownloadResponse::File(File {
													content: buffer.to_vec()
												}))
											})).await.unwrap();
										}

										reader.consume(length);
									};
								}
							} else {
								eprintln!("File does not exists");
							}
						});
					}
				}
				_ => {
					eprintln!("unknown error");
				}
			}
			Err(error) => {
				eprintln!("{}", error);
			}
		};

		Ok(Response::new(ReceiverStream::new(rx)))
	}
}