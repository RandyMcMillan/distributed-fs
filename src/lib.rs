pub mod service {
    tonic::include_proto!("api");
}

pub mod api;
pub mod behaviour;
pub mod constants;
pub mod entry;
pub mod event_loop;
pub mod node;
pub mod swarm;
