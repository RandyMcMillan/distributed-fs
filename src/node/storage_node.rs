use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use std::io::{BufReader, Read};
use std::path::Path;
use std::{fs, str};
use tokio::sync::{mpsc, oneshot};

use crate::api::utils::split_get_file_request;
use crate::behaviour::{
    FileRequest, FileRequestType, FileResponse, FileResponseType, GetFileResponse, ProvideResponse,
};
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::node::NodeType;
use crate::swarm::ManagedSwarm;

#[derive(Debug)]
pub struct StorageNode {
    // Request-response Request receiever
    requests_receiver: mpsc::Receiver<ReqResEvent>,
    // DHT events for eventloop sender
    dht_event_sender: mpsc::Sender<DhtEvent>,
    // State of stored chunks
    // storage_state: StorageState,
}

impl StorageNode {
    pub async fn new(swarm_addr: &str) -> Self {
        let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
        let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);

        let managed_swarm = ManagedSwarm::new(swarm_addr.parse().unwrap()).await;
        let event_loop = EventLoop::new(managed_swarm, requests_sender, dht_event_receiver);

        tokio::spawn(async move {
            event_loop.run().await;
        });

        Self {
            requests_receiver,
            dht_event_sender,
            // storage_state: Default::default(),
        }
    }

    pub async fn run_storage_node(mut self) {
        loop {
            tokio::select! {
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
                    FileResponse(FileResponseType::GetNodeTypeResponse(NodeType::StorageNode));

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
            FileRequestType::ProvideRequest(cids) => {
                let response =
                    FileResponse(FileResponseType::ProvideResponse(ProvideResponse::Success));

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

                let get_cids_reqs = split_get_file_request(cids);
                for get_cids in get_cids_reqs {
                    let request = FileRequest(FileRequestType::GetFileRequest(get_cids));

                    let (sender, receiver) = oneshot::channel();
                    self.dht_event_sender
                        .send(DhtEvent::SendRequest {
                            sender,
                            request,
                            peer,
                        })
                        .await
                        .unwrap();
                    let res = receiver.await.unwrap().unwrap();

                    match res.0 {
                        FileResponseType::GetFileResponse(GetFileResponse { content, cids }) => {
                            println!("{:?}, content blocks: {:?}", cids, content.len());

                            for (i, cid) in cids.iter().enumerate() {
                                let location = format!("./cache/{}", cid.to_string());
                                let path: &Path = Path::new(&location);

                                if path.exists() {
                                    continue;
                                }

                                match fs::write(path, &content[i]) {
                                    Err(_error) => {
                                        println!("error writing file");
                                    }
                                    _ => {}
                                };
                            }
                        }
                        _ => println!("Uknown error"),
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
