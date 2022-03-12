use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
	pub user: String,
	pub path: String,
	pub public: bool,
	pub read_users: Vec<String>
}