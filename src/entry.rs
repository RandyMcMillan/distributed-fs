use crate::service::{ApiChildren, ApiEntry};
use serde::{Deserialize, Serialize};
use libp2p::PeerId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub signature: String,
    pub owner: String,
    pub public: bool,
    pub providers: Vec<String>,
    pub read_users: Vec<String>,
    pub metadata: EntryMetaData,
    pub storage_nodes: Vec<String>
}

impl Entry {
    pub fn new(signature: String, public_key: String, entry: ApiEntry, storage_nodes: &Vec<PeerId>) -> Self {
        Self {
            signature,
            owner: public_key,
            public: entry.public,
            providers: Vec::new(),
            read_users: entry.read_users,
            metadata: EntryMetaData::new(entry.children, entry.name),
            storage_nodes: storage_nodes.into_iter().map(|peer_id| peer_id.to_string()).collect()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EntryMetaData {
    pub children: Vec<Children>,
    pub name: String,
}

impl EntryMetaData {
    fn new(children: Vec<ApiChildren>, name: String) -> Self {
        Self {
            name,
            children: children
                .iter()
                .map(|child| Children {
                    name: child.name.clone(),
                    r#type: child.r#type.clone(),
                    cids: child.cids.clone(),
                    size: child.size,
                    data: child.data.clone(),
                })
                .collect(),
        }
    }

    pub fn api_children(&self, location: Option<String>) -> Vec<ApiChildren> {
        if let Some(location) = location {
            return self
                .children
                .iter()
                .filter(|item| item.name.starts_with(&location))
                .map(|item| ApiChildren {
                    name: item.name.clone(),
                    r#type: item.r#type.clone(),
                    size: 0,
                    cids: item.cids.clone(),
                    data: item.data.clone(),
                })
                .collect::<Vec<ApiChildren>>();
        }

        self.children
            .iter()
            .map(|item| ApiChildren {
                name: item.name.clone(),
                r#type: item.r#type.clone(),
                size: 0,
                cids: item.cids.clone(),
                data: item.data.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Children {
    pub name: String,
    pub r#type: String,
    pub cids: Vec<String>,
    pub size: i32,
    pub data: Option<Vec<u8>>,
}
