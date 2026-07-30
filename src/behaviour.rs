use crate::constants::MAX_REQUEST_SIZE;
use async_std::io;
use async_trait::async_trait;
use futures::prelude::*;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaEvent};
use libp2p::request_response::{
    Behaviour as RequestResponse, Codec as RequestResponseCodec, Config as RequestResponseConfig,
    Event as RequestResponseEvent, Message as RequestResponseMessage, ProtocolSupport,
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    swarm::StreamProtocol,
    NetworkBehaviour,
};
use serde::{Deserialize, Serialize};
use std::{iter, str};

use crate::node::NodeType;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent", event_process = false)]
pub struct MyBehaviour {
    pub kademlia: Kademlia<MemoryStore>,
    pub mdns: Mdns,
    pub request_response: RequestResponse<FileExchangeCodec>,
}

impl MyBehaviour {
    pub fn create_req_res() -> RequestResponse<FileExchangeCodec> {
        RequestResponse::with_codec(
            FileExchangeCodec(),
            iter::once((StreamProtocol::new("/file-exchange/1"), ProtocolSupport::Full)),
            RequestResponseConfig::default(),
        )
    }
}

#[derive(Debug)]
pub enum OutEvent {
    Kademlia(KademliaEvent),
    Mdns(MdnsEvent),
    RequestResponse(RequestResponseEvent<FileRequest, FileResponse>),
}

impl From<RequestResponseEvent<FileRequest, FileResponse>> for OutEvent {
    fn from(event: RequestResponseEvent<FileRequest, FileResponse>) -> Self {
        Self::RequestResponse(event)
    }
}

impl From<KademliaEvent> for OutEvent {
    fn from(event: KademliaEvent) -> Self {
        Self::Kademlia(event)
    }
}

impl From<MdnsEvent> for OutEvent {
    fn from(event: MdnsEvent) -> Self {
        Self::Mdns(event)
    }
}

#[derive(Clone)]
pub struct FileExchangeCodec();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRequestType {
    GetFileRequest(Vec<String>),
    ProvideRequest(Vec<(String, i32)>),
    GetNodeTypeRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRequest(pub FileRequestType);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetFileResponse {
    pub content: Vec<Vec<u8>>,
    pub cids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvideResponse {
    Error(String),
    Success,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileResponseType {
    GetFileResponse(GetFileResponse),
    ProvideResponse(ProvideResponse),
    GetNodeTypeResponse(NodeType),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResponse(pub FileResponseType);

#[async_trait]
impl RequestResponseCodec for FileExchangeCodec {
    type Protocol = StreamProtocol;
    type Request = FileRequest;
    type Response = FileResponse;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(MAX_REQUEST_SIZE.try_into().unwrap())
            .read_to_end(&mut vec)
            .await?;

        let req: FileRequest = serde_json::from_str(str::from_utf8(&vec).unwrap()).unwrap();

        Ok(req)
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut vec = Vec::new();
        io.take(2_000_000)
            .read_to_end(&mut vec)
            .await?;

        let req: FileResponse = serde_json::from_str(str::from_utf8(&vec).unwrap()).unwrap();
        Ok(req)
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        FileRequest(d): FileRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = serde_json::to_vec(&d).unwrap();
        io.write_all(data.as_ref()).await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        FileResponse(d): FileResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = serde_json::to_vec(&d).unwrap();
        io.write_all(data.as_ref()).await?;

        Ok(())
    }
}
