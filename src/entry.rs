use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Entry {
	pub user: String,
	pub name: String,
	pub public: bool,
	pub read_users: Vec<String>,
	pub children: Vec<Children>
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Children {
	pub name: String,
	pub r#type: String,
	pub entry: String  
}

impl Entry {
	pub fn has_access(&self, username: String) -> bool {
		self.user == username || self.public || self.read_users.contains(&username) 
	}
}