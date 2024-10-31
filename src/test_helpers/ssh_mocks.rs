use async_trait::async_trait;
use crate::config::{Cluster, Node};
use crate::deps::SshManager;
use crate::ssh::{SshClient, SshClients, SshError, SshErrors};

pub struct MockSshManager{}

#[async_trait]
impl SshManager for MockSshManager {
    async fn node_connect(&self, cluster: &Cluster, node: &Node) -> Result<Box<dyn SshClient>, SshError> {
        todo!("implement me")
    }

    async fn cluster_connect(&self, cluster: &Cluster) -> (Option<SshClients>, Option<SshErrors>) {
        todo!("implement me")
    }
}