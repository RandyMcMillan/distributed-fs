use libp2p::kad::record::Key;
use std::str;
use tokio::net::TcpStream; 
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use secp256k1::{PublicKey, Secp256k1, SecretKey, Message};
use secp256k1::hashes::sha256;
use std::str::FromStr;
use futures::stream::StreamExt;
use std::fs;
use std::path::Path;
use tokio::sync::{mpsc, broadcast};
use tokio::sync::Mutex;
use std::sync::Arc;

use tonic::{transport::Server, Request, Response, Status, Code};
use crate::api::api_server::{Api, ApiServer};
use crate::api::{GetRequest, GetResponse, PutResponse, PutRequest, Entry, FileUploadRequest, FileUploadResponse, MetaData, File, file_upload_request::UploadRequest};

// use crate::entry::Entry;
// use crate::Dht;

// #[derive(Debug, Deserialize)]
// struct PutRecordRequest {
// 	entry: Entry,
// 	secret_key: String,
// 	public_key: String,
// }

// #[derive(Debug, Deserialize)]
// struct GetRecordQuery {
// 	location: String,
// 	secret_key: String,
// 	public_key: String,
// }

// #[derive(Debug, Deserialize, Serialize)]
// struct GetRecordResponse {
// 	found: bool,
// 	key: String,
// 	data: Option<Entry>,
// 	error: Option<String>
// }

// pub async fn handle_stream(mut stream: TcpStream, swarm: &mut Dht) -> Result<(), String> {
// 	let mut buffer = [0; 1024 * 8];
// 	let bytes_read = match stream.read(&mut buffer).await {
// 		Ok(b) => b,
// 		Err(_error) => return Err("Could not read bytes".to_string())
// 	};

// 	let mut headers = [httparse::EMPTY_HEADER; 64];
// 	let mut req = httparse::Request::new(&mut headers);
// 	match req.parse(&buffer) {
// 		Ok(..) => {},
// 		Err(_error) => return Err("Failed to parse headers".to_string())
// 	};


// 	let request = String::from_utf8_lossy(&buffer[..bytes_read]);
// 	let mut parts = request.split("\r\n\r\n");
// 	match parts.next() {
// 		Some(_val) => {},
// 		None => return Err("Expected body".to_string())
// 	};

// 	let body = match parts.next() {
// 		Some(val) => val.to_string(),
// 		None => "".to_string()
// 	};

// 	if body.len() == 0 {
// 		return Err("Got body of length 0".to_string())
// 	}

// 	if req.method.unwrap() == "POST" && req.path.unwrap() == "/put" {
// 		let put_request: PutRecordRequest = serde_json::from_str(&body).unwrap();
// 		let mut entry: Entry = put_request.entry;

// 		let (public_key, secret_key) = match check_user(put_request.secret_key, put_request.public_key) {
// 			Ok(pkey) => pkey,
// 			Err(error) => return Err(error)
// 		};

// 		entry.user = public_key;
// 		let value = serde_json::to_vec(&entry).unwrap();

// 		let secp = Secp256k1::new();
// 		let message = Message::from_hashed_data::<sha256::Hash>(format!("{}/{}", entry.user, entry.name).as_bytes());
// 		let sig = secp.sign_ecdsa(&message, &secret_key);

// 		let key: String = format!("e_{}", sig.to_string());

// 		stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: text/html\n\n{}", key.clone()).as_bytes()).await.unwrap();

// 		match swarm.put(Key::new(&key), value).await {
// 			Ok(_) => {
// 				println!("Success");
// 			},
// 			Err(err) => {
// 				eprintln!("{}", err);
// 				stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: text/html\n\n{}", key).as_bytes()).await.unwrap();
// 			}
// 		}
// 	} else if req.method.unwrap() == "GET" && req.path.unwrap() == "/get" {
// 		let get_request: GetRecordQuery = serde_json::from_str(&body).unwrap();
// 		let key = get_location_key(get_request.location.clone());

// 		let (public_key, _) = match check_user(get_request.secret_key, get_request.public_key) {
// 			Ok(pkey) => pkey,
// 			Err(error) => return handle_get_error(stream, key, error).await 
// 		};

// 		match swarm.get(&key).await {
// 			Ok(record) => {
// 				let entry: Entry = serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

// 				if entry.has_access(public_key) {
// 					let res = GetRecordResponse {
// 						key: str::from_utf8(&key.to_vec()).unwrap().to_string(),
// 						found: true,
// 						data: Some(entry),
// 						error: None
// 					};

// 					stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: application/json\n\n{}", serde_json::to_string(&res).unwrap()).as_bytes()).await.unwrap();
// 				} else {
// 					handle_get_error(stream, key.clone(), format!("Access to {:?} not allowed", key)).await;
// 				}
// 			},
// 			Err(_) => {
// 				handle_get_error(stream, key, "failed to get entry".to_string()).await;
// 			},
// 		};
// 	}

// 	Ok(())
// }

// async fn handle_get_error(mut stream: TcpStream, key: Key, error_message: String ) -> Result<(), String> {
// 	let res = GetRecordResponse {
// 		key: str::from_utf8(&key.to_vec()).unwrap().to_string(),
// 		found: false,
// 		data: None,
// 		error: Some(error_message.clone())
// 	};

// 	stream.write_all(format!("HTTP/1.1 200 OK\nContent-Type: application/json\n\n{}", serde_json::to_string(&res).unwrap()).as_bytes()).await.unwrap();

// 	Err(error_message)
// }

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

fn check_user(secret_key: String, public_key: String) -> Result<(String, SecretKey), String> {
	let secp = Secp256k1::new();
	let secret_key_parsed = match SecretKey::from_str(&secret_key) {
		Ok(skey) => skey,
		Err(error) => return Err(error.to_string())
	};
	let public_key_from_secret = PublicKey::from_secret_key(&secp, &secret_key_parsed);

	if public_key_from_secret.to_string() != public_key {
		return Err("Invalid keys".to_string())
	}
	
	Ok((public_key_from_secret.to_string(), secret_key_parsed))
}



#[derive(Debug)]
pub struct DhtGetRecordRequest {
	pub location: String
}

#[derive(Debug)]
pub struct DhtPutRecordRequest {
        pub public_key: PublicKey,
	pub entry: Entry,
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
	pub error: Option<String>
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
impl Api for MyApi {
	async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
		let dht_request = DhtRequestType::GetRecord(DhtGetRecordRequest {
			location: request.get_ref().location.to_owned()
		});

		self.mpsc_sender.send(dht_request).await;
		match self.broadcast_receiver.lock().await.recv().await {
			Ok(dht_response) => match dht_response {
                                DhtResponseType::GetRecord(dht_get_response) => {
                                        if let Some(error) = dht_get_response.error {
						return DhtGetRecordResponse::not_found();
                                        }

                                        Ok(Response::new(GetResponse {
                                                entry: dht_get_response.entry
                                        }))
                                }
                                _ => DhtGetRecordResponse::default()
                        }
			Err(error) => {
				eprintln!("Error {}", error);
                                DhtGetRecordResponse::default()
			}
		}
	}

	async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
		let signature: String = request.get_ref().signature.clone();
		let dht_request = DhtRequestType::PutRecord(DhtPutRecordRequest {
			public_key: PublicKey::from_str(request.metadata().get("public_key").unwrap().to_str().unwrap()).unwrap(),
			signature: signature.clone(),
			entry: request.into_inner().entry.unwrap(),
		});

		self.mpsc_sender.send(dht_request).await;
		match self.broadcast_receiver.lock().await.recv().await {
                        Ok(dht_response) => match dht_response {
                                DhtResponseType::PutRecord(dht_put_response) => {
                                        if let Some((code, message)) = dht_put_response.error {
						return Err(Status::new(code, message));
                                        }

					Ok(Response::new(PutResponse {
						key: signature.clone()
					}))
                                }
                                _ => {
					println!("unknown error");
					Ok(Response::new(PutResponse {
						key: signature.clone()
					}))
				}
                        }
                        Err(error) => {
				eprintln!("{}", error);
				Ok(Response::new(PutResponse {
					key: signature
				}))
                        }
                }
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
								}));
							}
						}
						Err(error) => {
							eprintln!("{}", error);
							return Ok(Response::new(FileUploadResponse {
								key: signature.unwrap()
							}));
						}
					};
                                }
                                UploadRequest::File(file) => {
                                        v.extend_from_slice(&file.content); 
                                        // v.extend_from_slice(&[10u8]); 
                                }
                        };
                }
                // v.pop();

		let path: &Path =  Path::new("./out");
		match fs::write(path, v) {
			Ok(_) => {
				Ok(Response::new(FileUploadResponse {
					key: signature.unwrap().clone()
				}))
			}
			Err(error) => {
				Err(Status::new(Code::Unknown, error.to_string()))
			}
		}
        }
}
