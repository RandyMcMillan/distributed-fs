use futures::stream::StreamExt;
use libp2p::kad::record::Key;
use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use std::io::{BufReader, Read};
use std::path::Path;
use std::{fs, str};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::utils::{get_cids_with_sizes, get_location_key, split_get_file_request};
use crate::api::{
    DhtGetRecordRequest, DhtGetRecordResponse, DhtPutRecordRequest, DhtPutRecordResponse,
    DhtRequestType, DhtResponseType,
};
use crate::behaviour::{
    FileRequest, FileRequestType, FileResponse, FileResponseType, GetFileResponse,
};
use crate::entry::Entry;
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::swarm::ManagedSwarm;

pub struct ApiHandler {
    // gRPC request receiver stream
    api_req_receiver_stream: ReceiverStream<DhtRequestType>,
    // gRPC response sender
    api_res_sender: broadcast::Sender<DhtResponseType>,
    // Request-response Request receiever
    requests_receiver: mpsc::Receiver<ReqResEvent>,
    // DHT events for eventloop sender
    dht_event_sender: mpsc::Sender<DhtEvent>,
    // State of stored chunks
    storage_state: StorageState
}

impl ApiHandler {
    pub fn new(
        api_req_receiver: mpsc::Receiver<DhtRequestType>,
        api_res_sender: broadcast::Sender<DhtResponseType>,
        managed_swarm: ManagedSwarm,
    ) -> Self {
        let api_req_receiver_stream = ReceiverStream::new(api_req_receiver);

        let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
        let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);
        let event_loop = EventLoop::new(managed_swarm, requests_sender, dht_event_receiver);

        tokio::spawn(async move {
            event_loop.run().await;
        });

        Self {
            api_req_receiver_stream,
            api_res_sender,
            requests_receiver,
            dht_event_sender,
            storage_state: Default::default()
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                data = self.api_req_receiver_stream.next() => {
                    match data {
                        Some(data) => {
                            match self.handle_api_event(data).await {
                                Err(error) => println!("{}", error),
                                _ => {}
                            };
                        }
                        _ => {}
                    }
                }
                req = self.requests_receiver.recv() => {
                    match req.unwrap() {
                        ReqResEvent::InboundRequest { request, channel, peer } => {
                            match self.handle_request_response(request, channel, peer).await {
                                Err(error) => eprint!("Error in handle_request_response: {}", error),
                                _ => {}
                            };
                        }
                    }
                }
            }
        }
    }

    pub async fn handle_api_event(&mut self, data: DhtRequestType) -> Result<(), String> {
        match data {
            DhtRequestType::GetRecord(DhtGetRecordRequest {
                signature,
                name,
                public_key,
            }) => {
                let loc = signature.clone();

                let (key, location, _signature) = get_location_key(signature.clone()).unwrap();

                let (sender, receiver) = oneshot::channel();
                self.dht_event_sender
                    .send(DhtEvent::GetRecord {
                        key: key.clone(),
                        sender,
                    })
                    .await
                    .unwrap();

                match receiver.await.unwrap() {
                    Ok(record) => {
                        let entry: Entry =
                            serde_json::from_str(&str::from_utf8(&record.value).unwrap()).unwrap();

                        self.api_res_sender
                            .send(DhtResponseType::GetRecord(DhtGetRecordResponse {
                                entry: Some(entry),
                                error: None,
                                location: Some(location),
                            }))
                            .unwrap();
                    }
                    Err(error) => {
                        self.api_res_sender
                            .send(DhtResponseType::GetRecord(DhtGetRecordResponse {
                                entry: None,
                                error: Some(error.to_string()),
                                location: None,
                            }))
                            .unwrap();
                    }
                };
            }
            DhtRequestType::PutRecord(DhtPutRecordRequest {
                entry,
                signature,
                public_key,
            }) => {
                println!("Put request");
                let key: String = format!("e_{}", signature.to_string());

                let entry = Entry::new(signature, public_key.to_string(), entry);
                let value = serde_json::to_vec(&entry).unwrap();

                let (sender, receiver) = oneshot::channel();
                self.dht_event_sender
                    .send(DhtEvent::PutRecord {
                        key: Key::new(&key.clone()),
                        sender,
                        value,
                    })
                    .await
                    .unwrap();
                let res = match receiver.await.unwrap() {
                    Ok(key) => {
                        println!("kad put request completed");
                        let (sender, receiver) = oneshot::channel();
                        self.dht_event_sender
                            .send(DhtEvent::GetProviders {
                                key: key.clone(),
                                sender,
                            })
                            .await
                            .unwrap();
                        let peers = receiver.await.unwrap().unwrap();
                        let key = String::from_utf8(key.clone().to_vec()).unwrap();

                        println!("got peers");

                        if !peers.is_empty() {
                            let cids_with_sizes = get_cids_with_sizes(entry.metadata.children);

                            let request =
                                FileRequest(FileRequestType::ProvideRequest(cids_with_sizes));

                            let (sender, receiver) = oneshot::channel();
                            self.dht_event_sender
                                .send(DhtEvent::SendRequest {
                                    sender,
                                    request,
                                    peer: peers[0],
                                })
                                .await
                                .unwrap();
                            
                            println!("Got provideRequest res");

                            match receiver.await.unwrap() {
                                Ok(_res) => {}
                                Err(error) => eprint!("Start providing err: {}", error),
                            };
                        }

                        println!("got here");

                        DhtResponseType::PutRecord(DhtPutRecordResponse {
                            signature: Some(key),
                            error: None,
                        })
                    }
                    Err(error) => DhtResponseType::PutRecord(DhtPutRecordResponse {
                        signature: None,
                        error: Some(error.to_owned()),
                    }),
                };

                self.api_res_sender.send(res).unwrap();
            }
        };

        Ok(())
    }

    pub async fn handle_request_response(
        &mut self,
        req: FileRequest,
        channel: ResponseChannel<FileResponse>,
        peer: PeerId,
    ) -> Result<(), String> {
        let FileRequest(r) = req;

        match r {
            FileRequestType::ProvideRequest(cids) => {
                let response = FileResponse(FileResponseType::ProvideResponse(
                    "Started providing".to_owned(),
                ));

                let (sender, receiver) = oneshot::channel();
                self.dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await
                    .unwrap();
                receiver.await.unwrap().unwrap();

                if !cids.is_empty() {
                    for req_cids in split_get_file_request(cids) {
                        println!("{:#?}", req_cids);
                        let request = FileRequest(FileRequestType::GetFileRequest(req_cids));

                        let (sender, receiver) = oneshot::channel();
                        self.dht_event_sender
                            .send(DhtEvent::SendRequest {
                                sender,
                                request,
                                peer,
                            })
                            .await
                            .unwrap();
                        match receiver.await.unwrap() {
                            Ok(response) => match response.0 {
                                FileResponseType::GetFileResponse(GetFileResponse {
                                    cids,
                                    content,
                                }) => {
                                    for (i, cid) in cids.iter().enumerate() {
                                        let p = format!("./cache/2/{}", cid);
                                        let path = Path::new(&p);

                                        match fs::write(path, content[i].clone()) {
                                            Err(error) => {
                                                eprint!("error while writing file...\n {}", error)
                                            }
                                            _ => {}
                                        };

                                        self.storage_state.add_pin(cid.to_string())
                                    }
                                }
                                _ => {}
                            },
                            Err(error) => eprint!("Error while sending request: {}", error),
                        }
                    }
                }
            }
            FileRequestType::GetFileRequest(cids) => {
                println!("Got filerequest");
                let mut content = Vec::new();

                for cid in cids.clone() {
                    let location = format!("./cache/{}", cid.clone());

                    let c = {
                        if Path::new(&location).exists() {
                            let f = fs::File::open(&location).unwrap();
                            let mut reader = BufReader::new(f);
                            let mut buffer = Vec::new();

                            reader.read_to_end(&mut buffer).unwrap();

                            buffer
                        } else {
                            println!("doesn't exists");
                            Vec::new()
                        }
                    };

                    content.push(c);
                }

                let response = FileResponse(FileResponseType::GetFileResponse(GetFileResponse {
                    content,
                    cids,
                }));

                let (sender, receiver) = oneshot::channel();
                self.dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await
                    .unwrap();
                match receiver.await.unwrap() {
                    Ok(_) => {}
                    Err(_error) => {
                        println!("{:?}", _error);
                    }
                };
            }
        }

        Ok(())
    }
}

#[derive(Default, Debug)]
struct StorageState {
    pinned: Vec<String>,
    pub need_list: Vec<String>
}

impl StorageState {
    fn add_pin(cid: String) {
        self.pinned.push(cid);
    }
}