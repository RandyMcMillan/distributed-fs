use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::{DhtRequestType, DhtResponseType};
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::handler::StorageState;
use crate::swarm::ManagedSwarm;

#[derive(Debug)]
enum NodeImp {
    ApiNode(ApiNode),
    StroageNode(StorageNode),
}

#[derive(Debug)]
pub struct Node(NodeImp);

impl From<ApiNode> for Node {
    fn from(imp: ApiNode) -> Self {
        Self(NodeImp::ApiNode(imp))
    }
}

impl From<StorageNode> for Node {
    fn from(imp: StorageNode) -> Self {
        Self(NodeImp::StroageNode(imp))
    }
}

impl Node {
    pub async fn new_api_node(swarm_addr: &str) -> Result<Node, String> {
        Ok(ApiNode::new(swarm_addr).await.into())
    }

    pub async fn new_storage_node(swarm_addr: &str) -> Result<Node, String> {
        Ok(StorageNode::new(swarm_addr).await.into())
    }
}

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
    pub async fn new(swarm_addr: &str) -> Self {
        let (api_req_sender, api_req_receiver) = mpsc::channel::<DhtRequestType>(32);
        let (api_res_sender, api_res_receiver) = broadcast::channel::<DhtResponseType>(32);

        let api_req_receiver_stream = ReceiverStream::new(api_req_receiver);

        let (requests_sender, requests_receiver) = mpsc::channel::<ReqResEvent>(32);
        let (dht_event_sender, dht_event_receiver) = mpsc::channel::<DhtEvent>(32);

        let managed_swarm = ManagedSwarm::new(swarm_addr.parse().unwrap()).await;
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

#[derive(Debug)]
pub struct StorageNode {
    // Request-response Request receiever
    requests_receiver: mpsc::Receiver<ReqResEvent>,
    // DHT events for eventloop sender
    dht_event_sender: mpsc::Sender<DhtEvent>,
    // State of stored chunks
    storage_state: StorageState,
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
            storage_state: Default::default(),
        }
    }
}
