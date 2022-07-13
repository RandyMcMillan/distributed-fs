use futures::stream::StreamExt;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;

use crate::api::{DhtRequestType, DhtResponseType, MyApi};
use crate::event_loop::{DhtEvent, EventLoop, ReqResEvent};
use crate::service::service_server::ServiceServer;
use crate::swarm::ManagedSwarm;

#[derive(Debug)]
pub struct ApiNode {
    // gRPC request receiver stream
    api_req_receiver_stream: ReceiverStream<DhtRequestType>,
    // gRPC request sender
    api_req_sender: mpsc::Sender<DhtRequestType>,
    // gRPC response sender
    api_res_sender: broadcast::Sender<DhtResponseType>,
    // gRPC response receiver
    api_res_receiver: broadcast::Receiver<DhtResponseType>,
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
            api_req_sender,
            api_res_sender,
            api_res_receiver,
            requests_receiver,
            dht_event_sender,
        }
    }

    pub async fn run_api(self, addr: SocketAddr) {
        tokio::spawn(async move {
            let api = MyApi {
                api_req_sender: self.api_req_sender,
                api_res_receiver: Arc::new(Mutex::new(self.api_res_receiver)),
            };
            let server = Server::builder().add_service(ServiceServer::new(api));

            // let addr = args[2].parse().unwrap();
            println!("Server listening on {}", addr);
            server.serve(addr).await.unwrap();
        });

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
}
