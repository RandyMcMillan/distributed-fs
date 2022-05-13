use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
     Kademlia,
     KademliaEvent,
};
use libp2p::{
    mdns::{Mdns, MdnsEvent},
    NetworkBehaviour, 
};
use libp2p::core::upgrade::{
	read_length_prefixed, write_length_prefixed, ProtocolName
};
use libp2p::request_response::{
	ProtocolSupport, RequestResponse, RequestResponseCodec, RequestResponseEvent,
};
use async_trait::async_trait;
use std::iter;
use async_std::io;
use futures::prelude::*;

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
		// println!("Request_response event: {:?}", event);
		Self::RequestResponse(event)
	}
}

impl From<KademliaEvent> for OutEvent {
	fn from(event: KademliaEvent) -> Self {
		// println!("Kademlia Event: {:?}", event);
		Self::Kademlia(event)
	}
}

impl From<MdnsEvent> for OutEvent {
	fn from(event: MdnsEvent) -> Self {
		// println!("MDNS: {:?}", event);
		Self::Mdns(event)
	}
}

// impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
// 	fn inject_event(&mut self, event: MdnsEvent) {
// 		println!("MDNS2: {:?}", event);
// 		if let MdnsEvent::Discovered(list) = event {
// 			for (peer_id, multiaddr) in list {
// 				println!("{:?}, {:?}", peer_id, multiaddr);
// 				self.kademlia.add_address(&peer_id, multiaddr);
// 			}
// 		}
// 	}
// }

// impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
// 	fn inject_event(&mut self, msg: KademliaEvent) {
// 		match msg {
// 			KademliaEvent::OutboundQueryCompleted { result, .. } => match result {
// 				QueryResult::GetRecord(Ok(_ok)) => {},
// 				QueryResult::GetRecord(Err(err)) => {
// 					eprintln!("Failed to get record: {:?}", err);
// 				},
// 				_ => {
// 					println!("\n{:?}\n", result);
// 				}
// 			},
// 			_ => {}
// 		}
// 	}
// }


#[derive(Debug, Clone)]
pub struct FileExchangeProtocol();
#[derive(Clone)]
pub struct FileExchangeCodec();
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequest(pub String);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(pub String);

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
		let vec = read_length_prefixed(io, 1_000_000).await?;

		if vec.is_empty() {
			return Err(io::ErrorKind::UnexpectedEof.into());
		}

		Ok(FileRequest(String::from_utf8(vec).unwrap()))
	}

	async fn read_response<T>(
		&mut self,
		_: &FileExchangeProtocol,
		io: &mut T,
	) -> io::Result<Self::Response>
	where
		T: AsyncRead + Unpin + Send,
	{
		let vec = read_length_prefixed(io, 1_000_000).await?;

		if vec.is_empty() {
			return Err(io::ErrorKind::UnexpectedEof.into());
		}

		Ok(FileResponse(String::from_utf8(vec).unwrap()))
	}

	async fn write_request<T>(
		&mut self,
		_: &FileExchangeProtocol,
		io: &mut T,
		FileRequest(data): FileRequest,
	) -> io::Result<()>
	where
		T: AsyncWrite + Unpin + Send,
	{
		write_length_prefixed(io, data).await?;
		io.close().await?;

		Ok(())
	}

	async fn write_response<T>(
		&mut self,
		_: &FileExchangeProtocol,
		io: &mut T,
		FileResponse(data): FileResponse,
	) -> io::Result<()>
	where
		T: AsyncWrite + Unpin + Send,
	{
		write_length_prefixed(io, data).await?;
		io.close().await?;

		Ok(())
	}
}
