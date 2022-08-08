use futures::stream::StreamExt;
use libp2p::kad::record::Key;
use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use std::io::{BufReader, Read};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::{fs, str};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;

use crate::api::utils::{get_cids_with_sizes, get_location_key};
use crate::api::{
    DhtGetRecordRequest, DhtGetRecordResponse, DhtPutRecordRequest, DhtPutRecordResponse,
    DhtRequestType, DhtResponseType, MyApi,
};
use crate::behaviour::{
    FileRequest, FileRequestType, FileResponse, FileResponseType, GetFileResponse, ProvideResponse,
};
use crate::entry::Entry;
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::node::NodeType;
use crate::service::service_server::ServiceServer;
use crate::swarm::ManagedSwarm;

#[derive(Debug)]
pub struct ApiNode {
    // gRPC request receiver stream
    api_req_receiver_stream: ReceiverStream<DhtRequestType>,
    // gRPC response sender
    api_res_sender: broadcast::Sender<DhtResponseType>,
    // Request-response Request receiever
    requests_receiver: mpsc::Receiver<ReqResEvent>,
    // DHT events for eventloop sender
    dht_event_sender: mpsc::Sender<DhtEvent>,
}

impl ApiNode {
    pub async fn new(swarm_addr: &str, api_addr: SocketAddr) -> Self {
        let (api_req_sender, api_req_receiver) = mpsc::channel::<DhtRequestType>(32);
        let (api_res_sender, api_res_receiver) = broadcast::channel::<DhtResponseType>(32);

        let api_req_receiver_stream = ReceiverStream::new(api_req_receiver);

        let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
        let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);

        let managed_swarm = ManagedSwarm::new(swarm_addr.parse().unwrap()).await;
        let event_loop = EventLoop::new(managed_swarm, requests_sender, dht_event_receiver);

        // Run swarm(kad) eventloop
        tokio::spawn(async move {
            event_loop.run().await;
        });

        // Run gRPC api server
        tokio::spawn(async move {
            let api = MyApi {
                api_req_sender,
                api_res_receiver: Arc::new(Mutex::new(api_res_receiver)),
            };
            let server = Server::builder().add_service(ServiceServer::new(api));

            println!("gRPC server listening on {}", api_addr);
            server.serve(api_addr).await.unwrap();
        });

        Self {
            api_req_receiver_stream,
            api_res_sender,
            requests_receiver,
            dht_event_sender,
        }
    }

    pub async fn run_api(mut self) {
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

    pub async fn handle_request_response(
        &mut self,
        req: FileRequest,
        channel: ResponseChannel<FileResponse>,
        peer: PeerId,
    ) -> Result<(), String> {
        let FileRequest(r) = req;

        match r {
            FileRequestType::GetNodeTypeRequest => {
                let response =
                    FileResponse(FileResponseType::GetNodeTypeResponse(NodeType::ApiNode));

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
            }
            FileRequestType::ProvideRequest(_cids) => {
                let response = FileResponse(FileResponseType::ProvideResponse(
                    ProvideResponse::Error("Not a storage node".to_owned()),
                ));
                let (sender, _receiver) = oneshot::channel();
                self.dht_event_sender
                    .send(DhtEvent::SendResponse {
                        sender,
                        response,
                        channel,
                    })
                    .await
                    .unwrap();
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
                        let (sender, receiver) = oneshot::channel();
                        self.dht_event_sender
                            .send(DhtEvent::GetStorageNodes { sender })
                            .await
                            .unwrap();
                        let peers = receiver.await.unwrap().unwrap();
                        println!("Storage Nodes: {:?}", peers);

                        if !peers.is_empty() {
                            let cids_with_sizes = get_cids_with_sizes(entry.metadata.children);

                            let request =
                                FileRequest(FileRequestType::ProvideRequest(cids_with_sizes));

                            for peer in peers {
                                let (sender, receiver) = oneshot::channel();
                                self.dht_event_sender
                                    .send(DhtEvent::SendRequest {
                                        sender,
                                        request: request.clone(),
                                        peer,
                                    })
                                    .await
                                    .unwrap();

                                match receiver.await.unwrap() {
                                    Ok(res) => match res.0 {
                                        FileResponseType::ProvideResponse(provide_response) => {
                                            match provide_response {
                                                ProvideResponse::Error(error) => {
                                                    eprint!("Start providing err: {}", error);
                                                }
                                                ProvideResponse::Success => {
                                                    println!(
                                                        "Started providing on peer: {:?}",
                                                        peer
                                                    );
                                                }
                                            }
                                        }
                                        _ => {}
                                    },
                                    Err(error) => eprint!("Start providing err: {}", error),
                                };
                            }
                        }

                        let key = String::from_utf8(key.clone().to_vec()).unwrap();
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
}
