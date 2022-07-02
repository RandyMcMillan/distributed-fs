// use futures::stream::StreamExt;
// use libp2p::kad::record::Key;
// use libp2p::request_response::ResponseChannel;
// use libp2p::PeerId;
// use std::io::{BufReader, Read};
// use std::path::Path;
// use std::{fs, str};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::handler::StorageState;
use crate::swarm::ManagedSwarm;
// use crate::entry::Entry;
use crate::api::{
    // DhtGetRecordRequest, DhtGetRecordResponse, DhtPutRecordRequest, DhtPutRecordResponse,
    DhtRequestType,
    DhtResponseType,
};
// use crate::behaviour::{
//     FileRequest, FileRequestType, FileResponse, FileResponseType, GetFileResponse,
// };
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
// use crate::api::utils::{get_cids_with_sizes, get_location_key, split_get_file_request};

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
    pub fn new(managed_swarm: ManagedSwarm) -> Self {
        let (api_req_sender, api_req_receiver) = mpsc::channel::<DhtRequestType>(32);
        let (api_res_sender, api_res_receiver) = broadcast::channel::<DhtResponseType>(32);

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
}

pub struct StorageNode {
    // Request-response Request receiever
    requests_receiver: mpsc::Receiver<ReqResEvent>,
    // DHT events for eventloop sender
    dht_event_sender: mpsc::Sender<DhtEvent>,
    // State of stored chunks
    storage_state: StorageState,
}

impl StorageNode {
    pub fn new(managed_swarm: ManagedSwarm) -> Self {
        let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
        let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);
        let event_loop = EventLoop::new(managed_swarm, requests_sender, dht_event_receiver);

        tokio::spawn(async move {
            event_loop.run().await;
        });

        Self {
            requests_receiver,
            dht_event_sender,
            storage_state: Default::default(),
        }
    }
}
