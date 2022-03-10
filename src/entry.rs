use serde::{Serialize};

#[derive(Debug, Serialize)]
pub struct Entry {
	pub user: String,
	pub filename: String,
}