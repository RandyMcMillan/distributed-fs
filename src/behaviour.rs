use crate::constants::MAX_REQUEST_SIZE;
use async_std::io;
use async_trait::async_trait;
use futures::prelude::*;
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaEvent};
use libp2p::request_response::{
    ProtocolSupport, RequestResponse, RequestResponseCodec, RequestResponseEvent,
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    NetworkBehaviour,
};
use serde::{Deserialize, Serialize};
use std::{iter, str};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent", event_process = false)]
pub struct MyBehaviour {
    pub kademlia: Kademlia<MemoryStore>,
    pub mdns: Mdns,
    pub request_response: RequestResponse<FileExchangeCodec>,
}

impl MyBehaviour {
    pub fn create_req_res() -> RequestResponse<FileExchangeCodec> {
        RequestResponse::new(
            FileExchangeCodec(),
            iter::once((FileExchangeProtocol(), ProtocolSupport::Full)),
            Default::default(),
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

#[derive(Debug, Clone)]
pub struct FileExchangeProtocol();

#[derive(Clone)]
pub struct FileExchangeCodec();


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileRequestType {
    GetFileRequest(Vec<String>),
    ProvideRequest(Vec<(String, i32)>),
    GetNodeTypeRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeTypes {
    Storage,
    Api
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
    Success
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileResponseType {
    GetFileResponse(GetFileResponse),
    ProvideResponse(ProvideResponse),
    GetNodeTypeResponse(NodeTypes),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResponse(pub FileResponseType);

impl ProtocolName for FileExchangeProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/file-exchange/1".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for FileExchangeCodec {
    type Protocol = FileExchangeProtocol;
    type Request = FileRequest;
    type Response = FileResponse;

    async fn read_request<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, MAX_REQUEST_SIZE.try_into().unwrap()).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        let req: FileRequest = serde_json::from_str(&str::from_utf8(&vec).unwrap()).unwrap();

        Ok(req)
    }

    async fn read_response<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 2_000_000).await.unwrap();

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        let req: FileResponse = serde_json::from_str(&str::from_utf8(&vec).unwrap()).unwrap();
        Ok(req)
    }

    async fn write_request<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
        FileRequest(d): FileRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // println!("Write request: {:?}", d);
        let data = serde_json::to_vec(&d).unwrap();

        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
        FileResponse(d): FileResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // println!("Write response: {:?}", d);
        let data = serde_json::to_vec(&d).unwrap();

        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }
}
