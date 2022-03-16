use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
	pub user: String,
	pub name: String,
	pub public: bool,
	pub read_users: Vec<String>,
	pub children: Vec<Children>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Children {
	pub name: String,
	pub r#type: String,
	pub entry: String  
}