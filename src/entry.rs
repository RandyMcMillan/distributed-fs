use serde::{Serialize, Deserialize};
use crate::service::{ApiEntry, ApiChildren};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Entry {
	pub signature: String,
	pub owner: String,
	pub public: bool,
	pub providers: Vec<String>,
	pub read_users: Vec<String>,
	pub metadata: EntryMetaData,
}


impl Entry {
	pub fn new(signature: String, public_key: String, entry: ApiEntry) -> Self {
		Self {
			signature,
			owner: public_key,
			public: entry.public,
			providers: Vec::new(),
			read_users: entry.read_users,
			metadata: EntryMetaData::new(entry.children, entry.name)
		}
	}
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EntryMetaData {
	pub children: Vec<Children>,
	pub name: String
}

impl EntryMetaData {
	fn new(children: Vec<ApiChildren>, name: String) -> Self {
		Self {
			name,
			children: children.iter().map(|child| Children {
				name: child.name.clone(),
				r#type: child.r#type.clone(),
				cid: child.cid.clone()
			}).collect()
		}
	}
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Children {
	pub name: String,
	pub r#type: String,
	pub cid: Option<String>
}
