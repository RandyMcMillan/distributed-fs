use futures::StreamExt;
use libp2p::request_response::{
    Event as RequestResponseEvent, Message as RequestResponseMessage,
    OutboundRequestId as RequestId, ResponseChannel,
};
use libp2p::swarm::SwarmEvent;
use libp2p::{
    kad::{
        Event as KademliaEvent, GetProvidersOk, GetRecordOk, PutRecordOk, QueryResult, Record,
        RecordKey as Key,
    },
    mdns::Event as MdnsEvent,
};
use libp2p::PeerId;
use std::collections::HashMap;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

use crate::behaviour::{FileRequest, FileRequestType, FileResponse, FileResponseType, OutEvent};
use crate::node::NodeType;
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
    GetStorageNodes {
        sender: oneshot::Sender<Result<Vec<PeerId>, String>>
    },
}

#[derive(Debug)]
struct Ledger {
    score: u16,
    node_type: NodeType,
}

pub struct EventLoop {
    managed_swarm: ManagedSwarm,
    requests_sender: mpsc::Sender<ReqResEvent>,
    events_receiver: mpsc::Receiver<DhtEvent>,
    ledgers: HashMap<PeerId, Ledger>,
    pending_requests:
        HashMap<RequestId, oneshot::Sender<Result<FileResponse, Box<dyn Error + Send>>>>,
    pending_kademlia_queries:
        HashMap<libp2p::kad::QueryId, oneshot::Sender<Result<QueryResult, String>>>,
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
            pending_kademlia_queries: Default::default(),
        }
    }

    pub async fn run(mut self) {
        let test: Vec<String> = Vec::new();
        loop {
            if !test.is_empty() {
                println!("test");
            }

            tokio::select! {
                swarm_event = self.managed_swarm.0.select_next_some() => {
                    match swarm_event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("Listening on {:?}" , address);
                        }
                        SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
                            for (peer_id, multiaddr) in list {
                                self.managed_swarm.0.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                                self.ledgers.insert(peer_id, Ledger {
                                    score: 0,
                                    node_type: NodeType::ApiNode
                                });

                                let (sender, _receiver) = oneshot::channel();
                                let request = FileRequest(FileRequestType::GetNodeTypeRequest);

                                self.send_request(peer_id, request, sender).await.unwrap();
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
                            for (peer_id, multiaddr) in list {
                                println!("expired {:?}" , peer_id);
                                self.managed_swarm.0.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr)
                                    .expect("Error removing address");
                                self.ledgers.remove(&peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::RequestResponse(
                            RequestResponseEvent::Message { message, peer, .. },
                        )) => {
                            match message {
                                RequestResponseMessage::Response { response, request_id } => {
                                    match response.0 {
                                        FileResponseType::GetNodeTypeResponse(node_type) => {
                                            println!("{:?}: Node Type: {:?}" , peer, node_type);
                                            self.ledgers.insert(peer, Ledger{
                                                score: 0,
                                                node_type
                                            });
                                            println!("{:?}" , self.ledgers);
                                        }
                                        _ => {
                                            match self.pending_requests.remove(&request_id) {
                                                Some(sender) => {
                                                    sender.send(Ok(response)).unwrap();
                                                },
                                                None => {
                                                    eprintln!("Request not found: {}" , request_id);
                                                }
                                            };
                                        }
                                    };
                                }
                                RequestResponseMessage::Request { request, channel, .. } => {
                                    self.requests_sender.send(
                                        ReqResEvent::InboundRequest { request, channel, peer }
                                    ).await.unwrap();
                                }
                            }
                        }
                        SwarmEvent::Behaviour(OutEvent::Kademlia(e)) => {
                            match e {
                                KademliaEvent::OutboundQueryProgressed { id, result, .. } => {
                                    if let Some(sender) = self.pending_kademlia_queries.remove(&id) {
                                        sender.send(Ok(result)).unwrap();
                                    }
                                }
                                KademliaEvent::RoutingUpdated { peer, is_new_peer, addresses: _, old_peer: _, bucket_range: _ } => {
                                    if is_new_peer {
                                        println!("New peer in Kademlia routing table: {:?}" , peer);
                                        self.ledgers.insert(peer, Ledger {
                                            score: 0,
                                            node_type: NodeType::ApiNode // Default to ApiNode, will be updated by GetNodeTypeRequest
                                        });
                                        let (sender, _receiver) = oneshot::channel();
                                        let request = FileRequest(FileRequestType::GetNodeTypeRequest);
                                        self.send_request(peer, request, sender).await.unwrap();
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    };
                }
                dht_event = self.events_receiver.recv() => {
                    if let  Some(dht_event) = dht_event {
                        match dht_event {
                            DhtEvent::GetProviders { key, sender } => {
                                let query_id = self.managed_swarm.get_providers(key);
                                let (new_sender, new_receiver) = oneshot::channel();
                                self.pending_kademlia_queries.insert(query_id, new_sender);

                                let original_sender = sender;
                                tokio::spawn(async move {
                                    match new_receiver.await {
                                        Ok(Ok(QueryResult::GetProviders(Ok(
                                            GetProvidersOk::FoundProviders { providers, .. },
                                        )))) => {
                                            original_sender
                                                .send(Ok(providers.into_iter().collect()))
                                                .unwrap();
                                        }
                                        Ok(Ok(QueryResult::GetProviders(Ok(
                                            GetProvidersOk::FinishedWithNoAdditionalRecord {
                                                closest_peers,
                                            },
                                        )))) => {
                                            original_sender.send(Ok(closest_peers)).unwrap();
                                        }
                                        Ok(Ok(QueryResult::GetProviders(Err(e)))) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Ok(Err(e)) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Err(e) => {
                                            original_sender.send(Err(format!("Channel receive error: {}", e))).unwrap();
                                        }
                                        Ok(other_result) => {
                                            original_sender.send(Err(format!("Unexpected QueryResult for GetProviders: {:?}", other_result))).unwrap();
                                        }
                                    }
                                });
                            }
                            DhtEvent::GetRecord { key, sender } => {
                                let query_id = self.managed_swarm.get(key);
                                let (new_sender, new_receiver) = oneshot::channel();
                                self.pending_kademlia_queries.insert(query_id, new_sender);

                                let original_sender = sender;
                                tokio::spawn(async move {
                                    match new_receiver.await {
                                        Ok(Ok(QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(
                                            record,
                                        ))))) => {
                                            original_sender.send(Ok(record.record)).unwrap();
                                        }
                                        Ok(Ok(QueryResult::GetRecord(Ok(
                                            GetRecordOk::FinishedWithNoAdditionalRecord {
                                                cache_candidates,
                                            },
                                        )))) => {
                                            original_sender.send(Err(format!(
                                                "record not found; cache candidates: {:?}",
                                                cache_candidates
                                            ))).unwrap();
                                        }
                                        Ok(Ok(QueryResult::GetRecord(Err(e)))) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Ok(Err(e)) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Err(e) => {
                                            original_sender.send(Err(format!("Channel receive error: {}", e))).unwrap();
                                        }
                                        Ok(other_result) => {
                                            original_sender.send(Err(format!("Unexpected QueryResult for GetRecord: {:?}", other_result))).unwrap();
                                        }
                                    }
                                });
                            }
                            DhtEvent::PutRecord { key, sender, value } => {
                                let query_id = self.managed_swarm.put(key, value);
                                let (new_sender, new_receiver) = oneshot::channel();
                                self.pending_kademlia_queries.insert(query_id, new_sender);

                                let original_sender = sender;
                                tokio::spawn(async move {
                                    match new_receiver.await {
                                        Ok(Ok(QueryResult::PutRecord(Ok(PutRecordOk { key })))) => {
                                            original_sender.send(Ok(key)).unwrap();
                                        }
                                        Ok(Ok(QueryResult::PutRecord(Err(e)))) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Ok(Err(e)) => {
                                            original_sender.send(Err(e.to_string())).unwrap();
                                        }
                                        Err(e) => {
                                            original_sender.send(Err(format!("Channel receive error: {}", e))).unwrap();
                                        }
                                        Ok(other_result) => {
                                            original_sender.send(Err(format!("Unexpected QueryResult for PutRecord: {:?}", other_result))).unwrap();
                                        }
                                    }
                                });
                            }
                            DhtEvent::SendRequest { sender, request, peer } => {
                                self.send_request(peer, request, sender).await.unwrap();
                            }
                            DhtEvent::SendResponse { sender, response, channel } => {
                                sender.send(Ok(())).unwrap();
                                self.send_response(response, channel).await.unwrap();
                            }
                            DhtEvent::GetStorageNodes { sender } => {
                                sender.send(self.get_storage_nodes().await).unwrap()
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

    pub async fn get_storage_nodes(&mut self) -> Result<Vec<PeerId>, String> {
        let mut storage_nodes = Vec::new();
        let mut discovered_nodes = Vec::new();
        for (&peer_id, ledger) in self.ledgers.iter() {
            discovered_nodes.push(peer_id);
            if let NodeType::StorageNode = ledger.node_type {
                storage_nodes.push(peer_id);

                if storage_nodes.len() >= 3 {
                    break;
                }
            }
        }

        if storage_nodes.is_empty() {
            println!("No confirmed storage nodes yet; falling back to discovered peers");
            return Ok(discovered_nodes.into_iter().take(3).collect());
        }

        Ok(storage_nodes)
    }
}