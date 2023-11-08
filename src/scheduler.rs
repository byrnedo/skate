use std::cmp::Ordering;
use std::error::Error;
use async_trait::async_trait;
use crate::config::Cluster;
use crate::scheduler::Status::{Error as ScheduleError, Scheduled};
use crate::skate::SupportedResources;
use crate::skatelet::PodmanPodInfo;
use crate::ssh::{HostInfoResponse, SshClients};
use crate::state::state::{ClusterState, NodeState};


#[derive(Debug)]
pub enum Status {
    Scheduled(String),
    Error(String),
}

#[derive(Debug)]
pub struct ScheduleResult {
    pub object: SupportedResources,
    pub node_name: String,
    pub status: Status,
}

#[async_trait(? Send)]
pub trait Scheduler {
    async fn schedule(&self, conns: SshClients, state: &ClusterState, objects: Vec<SupportedResources>) -> Result<Vec<ScheduleResult>, Box<dyn Error>>;
}

pub struct DefaultScheduler {}

#[derive(Debug, Clone)]
struct ResourceAndNode<T> {
    resource: T,
    node: NodeState,
}

struct ApplyPlan {
    pub current: Option<ExistingResource>,
    pub next: Option<NodeState>,
}

#[derive(Debug, Clone)]
enum ExistingResource {
    Pod(ResourceAndNode<PodmanPodInfo>),
    Deployment(ResourceAndNode<Vec<PodmanPodInfo>>),
}

impl DefaultScheduler {
    // returns tuple of (Option(prev node), Option(new node))
    fn plan(state: &ClusterState, object: &SupportedResources) -> ApplyPlan {
        let existing_resource = match object {
            SupportedResources::Pod(p) => {
                let name = p.metadata.name.clone().unwrap_or("".to_string());
                let ns = p.metadata.namespace.clone().unwrap_or("".to_string());
                state.locate_pod(&name, &ns).map(|(r, n)| {
                    ExistingResource::Pod(ResourceAndNode { node: n.clone(), resource: r })
                })
            }
            SupportedResources::Deployment(d) => {
                let name = d.metadata.name.clone().unwrap_or("".to_string());
                let ns = d.metadata.namespace.clone().unwrap_or("".to_string());
                state.locate_deployment(&name, &ns).map(|(r, n)| {
                    ExistingResource::Deployment(ResourceAndNode { node: n.clone(), resource: r })
                })
            }
        };
        let current_node = match &existing_resource {
            Some(e) => match e {
                ExistingResource::Deployment(r) => Some(r.node.clone()),
                ExistingResource::Pod(r) => Some(r.node.clone()),
            },
            None => None
        };
        // naive - picks node with fewest pods
        let next = state.nodes.iter().fold(current_node, |maybe_prev_node, node| {
            let node_pods = node.clone().host_info.and_then(|h| {
                h.system_info.and_then(|si| {
                    si.pods.and_then(|p| Some(p.len()))
                })
            }).unwrap_or(0);

            maybe_prev_node.and_then(|prev_node| {
                prev_node.host_info.clone().and_then(|h| {
                    h.system_info.and_then(|si| {
                        si.pods.and_then(|prev_pods| {
                            match prev_pods.len().cmp(&node_pods) {
                                Ordering::Less => Some(prev_node.clone()),
                                Ordering::Equal => Some(node.clone()),
                                Ordering::Greater => Some(node.clone()),
                            }
                        })
                    })
                })
            }).or_else(|| Some(node.clone()))
        });
        ApplyPlan {
            current: existing_resource,
            next,
        }
    }

    async fn remove_existing(conns: &SshClients, resource: ExistingResource) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn schedule_one(conns: &SshClients, state: &ClusterState, object: SupportedResources) -> ScheduleResult {
        let serialized = match serde_yaml::to_string(&object).or_else(|err|
            Err(ScheduleResult {
                object: object.clone(),
                node_name: "".to_string(),
                status: ScheduleError(format!("{}", err)),
            })
        ) {
            Ok(s) => s,
            Err(sr) => return sr
        };

        let plan = Self::plan(state, &object);
        let next_node = match plan.next {
            Some(node) => node,
            None => return ScheduleResult {
                object,
                node_name: "".to_string(),
                status: ScheduleError("failed to find schedulable node".to_string()),
            }
        };

        let cleanup_result = match plan.current {
            Some(e) => Self::remove_existing(conns, e).await,
            None => Ok(())
        };
        match cleanup_result {
            Ok(_) => {}
            Err(e) => eprintln!("failed to cleanup existing resource: {}", e)
        }


        let client = conns.find(&next_node.node_name).unwrap();


        println!("scheduling {} on node {}", object, next_node.node_name.clone());
        let result = client.apply_resource(&serialized).await;
        ScheduleResult {
            object,
            node_name: next_node.node_name.clone(),
            status: match result {
                Ok((stdout, stderr)) => {
                    let mut builder = String::new();
                    builder.push_str(&stdout);
                    if stderr.len() > 0 {
                        builder.push_str(&format!(" ( stderr: {} )", stderr))
                    }
                    Scheduled(format!("{}", builder.replace("\n", "\\n")))
                }
                Err(err) => ScheduleError(err.to_string())
            },
        }
    }
}

#[async_trait(? Send)]
impl Scheduler for DefaultScheduler {
    async fn schedule(&self, conns: SshClients, state: &ClusterState, objects: Vec<SupportedResources>) -> Result<Vec<ScheduleResult>, Box<dyn Error>> {
        let mut results: Vec<ScheduleResult> = vec![];
        for object in objects {
            match object {
                SupportedResources::Pod(_) | SupportedResources::Deployment(_) => {
                    let result = Self::schedule_one(&conns, state, object.clone()).await;
                    results.push(result)
                }
            }
        }
        Ok(results)
    }
}
