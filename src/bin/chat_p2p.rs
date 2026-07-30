use ::futures::StreamExt;
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    mdns::{tokio::Behaviour as Mdns, Event as MdnsEvent},
    swarm::NetworkBehaviour,
    Multiaddr, PeerId, SwarmBuilder,
};
use libp2p_swarm::SwarmEvent;
use std::error::Error;
use tokio::io::{self, AsyncBufReadExt};

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "OutEvent", prelude = "libp2p_swarm::derive_prelude")]
struct MyBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
}

#[derive(Debug)]
enum OutEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

impl From<FloodsubEvent> for OutEvent {
    fn from(event: FloodsubEvent) -> Self {
        Self::Floodsub(event)
    }
}

impl From<MdnsEvent> for OutEvent {
    fn from(event: MdnsEvent) -> Self {
        Self::Mdns(event)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let topic = Topic::new("chat");
    let subscription_topic = topic.clone();

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            Default::default(),
            (libp2p::tls::Config::new, libp2p::noise::Config::new),
            libp2p::yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(move |key| {
            let local_peer_id = PeerId::from(key.public());
            let mdns = Mdns::new(libp2p::mdns::Config::default(), local_peer_id).unwrap();
            let mut floodsub = Floodsub::new(local_peer_id);
            floodsub.subscribe(subscription_topic.clone());

            MyBehaviour { floodsub, mdns }
        })
        .unwrap()
        .build();

    if let Some(to_dial) = std::env::args().nth(1) {
        let addr: Multiaddr = to_dial.parse()?;
        swarm.dial(addr)?;
    }

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        tokio::select! {
            line = stdin.next_line() => {
                if let Some(line) = line? {
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .publish(topic.clone(), line.into_bytes());
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(message))) => {
                        println!(
                            "Received: '{:?}' from {:?}",
                            String::from_utf8_lossy(&message.data),
                            message.source
                        );
                    }
                    SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
                        for (peer, _) in list {
                            swarm.behaviour_mut().floodsub.add_node_to_partial_view(peer);
                        }
                    }
                    SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
                        for (peer, _) in list {
                            if !swarm.behaviour().mdns.discovered_nodes().any(|node| node == &peer) {
                                swarm.behaviour_mut().floodsub.remove_node_from_partial_view(&peer);
                            }
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {:?}", address);
                    }
                    _ => {}
                }
            }
        }
    }
}
