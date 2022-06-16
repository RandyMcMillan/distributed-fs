use futures::StreamExt;
use libp2p::kad::record::Key;
use libp2p::kad::Record;
use libp2p::mdns::MdnsEvent;
use libp2p::request_response::{
    RequestId, RequestResponseEvent, RequestResponseMessage, ResponseChannel,
};
use libp2p::swarm::SwarmEvent;
use libp2p::PeerId;
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::behaviour::{FileRequest, FileResponse, OutEvent};
use crate::swarm::ManagedSwarm;

#[derive(Debug)]
pub enum ReqResEvent {
    InboundRequest {
        request: FileRequest,
        channel: ResponseChannel<FileResponse>,
        peer: PeerId,
    },
}

#[derive(Debug)]
pub enum DhtEvent {
    GetProviders {
        key: Key,
        sender: oneshot::Sender<Result<Vec<PeerId>, String>>,
    },
    StartProviding {
        key: Key,
        sender: oneshot::Sender<Result<Key, String>>,
    },
    GetRecord {
        key: Key,
        sender: oneshot::Sender<Result<Record, String>>,
    },
    PutRecord {
        key: Key,
        value: Vec<u8>,
        sender: oneshot::Sender<Result<Key, String>>,
    },
    SendRequest {
        peer: PeerId,
        request: FileRequest,
        sender: oneshot::Sender<Result<FileResponse, String>>,
    },
    SendResponse {
        channel: ResponseChannel<FileResponse>,
        response: FileResponse,
        sender: oneshot::Sender<Result<(), String>>,
    },
}

pub struct EventLoop {
    managed_swarm: ManagedSwarm,
    requests_sender: mpsc::Sender<ReqResEvent>,
    events_receiver: mpsc::Receiver<DhtEvent>,
    ledgers: HashMap<PeerId, u16>,
    pending_requests:
        HashMap<RequestId, oneshot::Sender<Result<FileResponse, Box<dyn Error + Send>>>>,
}

impl EventLoop {
    pub fn new(
        managed_swarm: ManagedSwarm,
        requests_sender: mpsc::Sender<ReqResEvent>,
        events_receiver: mpsc::Receiver<DhtEvent>,
    ) -> Self {
        Self {
            managed_swarm,
            requests_sender,
            events_receiver,
            ledgers: Default::default(),
            pending_requests: Default::default(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                swarm_event = self.managed_swarm.0.select_next_some() => {
                    match swarm_event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("Listening on {:?}", address);
                        }
                        SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
                            for (peer_id, multiaddr) in list {
                                self.managed_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                                self.ledgers.entry(peer_id).or_insert(0);
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
                            for (peer_id, multiaddr) in list {
                                println!("expired {:?}", peer_id);
                                self.managed_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
                                    .expect("Error removing address");
                                self.ledgers.remove(&peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::RequestResponse(
                            RequestResponseEvent::Message { message, peer },
                        )) => {
                            match message {
                                RequestResponseMessage::Response { response, request_id } => {
                                    match self.pending_requests.remove(&request_id) {
                                        Some(sender) => {
                                            sender.send(Ok(response)).unwrap();
                                        },
                                        None => {
                                            eprint!("Request not found: {}", request_id);
                                        }
                                    };
                                }
                                RequestResponseMessage::Request { request, channel, .. }  => {
                                    self.requests_sender.send(
                                        ReqResEvent::InboundRequest { request, channel, peer }
                                    ).await.unwrap();
                                }
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::Kademlia(_e)) => {}
                        _ => {}
                    };
                }
                dht_event = self.events_receiver.recv() => {
                    if let  Some(dht_event) = dht_event {
                        match dht_event {
                            DhtEvent::GetProviders { key, sender } => {
                                sender.send(self.managed_swarm.get_providers(key).await).unwrap();
                            }
                            DhtEvent::StartProviding { key, sender } => {
                                sender.send(self.managed_swarm.start_providing(key).await).unwrap();
                            }
                            DhtEvent::GetRecord { key, sender } => {
                                sender.send(self.managed_swarm.get(key).await).unwrap();
                            }
                            DhtEvent::PutRecord { key, sender, value } => {
                                sender.send(self.managed_swarm.put(key, value).await).unwrap();
                            }
                            DhtEvent::SendRequest { sender, request, peer } => {
                                self.send_request(peer, request, sender).await.unwrap();
                            }
                            DhtEvent::SendResponse { sender, response, channel } => {
                                sender.send(Ok(())).unwrap();
                                self.send_response(response, channel).await.unwrap();
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn send_request(
        &mut self,
        peer: PeerId,
        request: FileRequest,
        sender: oneshot::Sender<Result<FileResponse, String>>,
    ) -> Result<(), String> {
        let (res_sender, receiver) = oneshot::channel();
        let request_id = self
            .managed_swarm
            .send_request(peer, request)
            .await
            .unwrap();

        self.pending_requests.insert(request_id, res_sender);
        tokio::spawn(async move {
            let res = receiver.await.unwrap();
            match res {
                Ok(r) => sender.send(Ok(r)).unwrap(),
                Err(_r) => sender.send(Err("some error".to_owned())).unwrap(),
            };
        });

        Ok(())
    }

    pub async fn send_response(
        &mut self,
        response: FileResponse,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<(), String> {
        let behaviour = self.managed_swarm.0.behaviour_mut();

        behaviour
            .request_response
            .send_response(channel, response)
            .unwrap();

        Ok(())
    }
}
