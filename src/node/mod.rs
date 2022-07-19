pub mod api_node;
pub mod storage_node;

use api_node::ApiNode;
use storage_node::StorageNode;

#[derive(Debug)]
enum NodeImp {
    ApiNode(ApiNode),
    StorageNode(StorageNode),
}

#[derive(Debug)]
pub struct Node(NodeImp);

impl From<ApiNode> for Node {
    fn from(imp: ApiNode) -> Self {
        Self(NodeImp::ApiNode(imp))
    }
}

impl From<StorageNode> for Node {
    fn from(imp: StorageNode) -> Self {
        Self(NodeImp::StorageNode(imp))
    }
}

impl Node {
    pub async fn new_api_node(swarm_addr: &str, api_addr: &str) -> Result<Node, String> {
        Ok(ApiNode::new(swarm_addr, api_addr.parse().unwrap()).await.into())
    }

    pub async fn new_storage_node(swarm_addr: &str) -> Result<Node, String> {
        Ok(StorageNode::new(swarm_addr).await.into())
    }

    pub async fn run(self) {
        match self.0 {
            NodeImp::ApiNode(node) => {
                node.run_api().await;
            }
            NodeImp::StorageNode(node) => {
                node.run_storage_node().await;
            }
        }
    }
}
