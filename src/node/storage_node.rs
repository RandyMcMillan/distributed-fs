use tokio::sync::mpsc;

use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::handler::StorageState;
use crate::swarm::ManagedSwarm;

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

    pub async fn run_storage_node(self) {
        // loop {
        //     tokio::select! {
        //         req = self.requests_receiver.recv() => {
        //             match req.unwrap() {
        //                 ReqResEvent::InboundRequest { request, channel, peer } => {
        //                     match self.handle_request_response(request, channel, peer).await {
        //                         Err(error) => eprint!("Error in handle_request_response: {}", error),
        //                         _ => {}
        //                     };
        //                 }
        //             }
        //         }
        //     }
        // }
    }
}
