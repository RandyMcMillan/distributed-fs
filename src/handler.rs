use futures::stream::StreamExt;
use libp2p::kad::record::Key;
use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use std::io::{BufReader, Read};
use std::path::Path;
use std::{fs, str};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::api;
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

                let (key, location, _signature) = api::get_location_key(signature.clone()).unwrap();

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

                        if !peers.is_empty() {
                            let cids = entry
                                .metadata
                                .children
                                .iter()
                                .filter(|item| item.r#type == "file")
                                .map(|item| item.cid.as_ref().unwrap().clone())
                                .collect::<Vec<String>>();
                            println!("cids: {:?}", cids);

                            let request = FileRequest(FileRequestType::ProvideRequest(cids));

                            let (sender, receiver) = oneshot::channel();
                            self.dht_event_sender
                                .send(DhtEvent::SendRequest {
                                    sender,
                                    request,
                                    peer: peers[0],
                                })
                                .await
                                .unwrap();

                            match receiver.await.unwrap() {
                                Ok(_res) => {}
                                Err(error) => eprint!("Start providing err: {}", error),
                            };
                        }

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
                println!("{:#?}", cids);
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
                    let request = FileRequest(FileRequestType::GetFileRequest(cids));

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
                            FileResponseType::GetFileResponse(GetFileResponse { cids, content }) => {
                                for (i, cid) in cids.iter().enumerate() {
                                    let p = format!("./cache/2/{}", cid);
                                    let path = Path::new(&p);

                                    match fs::write(path, content[i].clone()) {
                                        Err(error) => {
                                            eprint!("error while writing file...\n {}", error)
                                        }
                                        _ => {}
                                    };
                                }
                            }
                            _ => {}
                        },
                        Err(error) => eprint!("Error while sending request: {}", error),
                    }
                }
            }
            FileRequestType::GetFileRequest(cids) => {
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
                    Err(_error) => {}
                };
            }
        }

        Ok(())
    }
}
