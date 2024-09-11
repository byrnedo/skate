use std::collections::HashMap;
use chrono::Local;
use itertools::Itertools;
use crate::get::{GetObjectArgs, Lister};
use crate::get::lister::NameFilters;
use crate::skatelet::SystemInfo;
use crate::skatelet::system::podman::{PodmanPodInfo, PodmanPodStatus};
use crate::state::state::ClusterState;
use crate::util::{age, NamespacedName};
use tabled::{builder::Builder, settings::{style::Style}, Tabled};

pub(crate) struct DeploymentLister {}

#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct DeploymentListItem {
    pub namespace: String,
    pub name: String,
    pub ready: String,
    pub up_to_date: String,
    pub available: String,
    pub age: String,
}

impl Lister<DeploymentListItem> for DeploymentLister {
    fn selector(&self, si: &SystemInfo, ns: &str, id: &str) -> Vec<DeploymentListItem> {
        // .si.pods.clone().unwrap_or_default().into_iter().filter_map(|p| {
        //     let deployment = p.labels.get("skate.io/deployment").unwrap_or(&"".to_string()).clone();
        //     if deployment == "" {
        //         return None;
        //     }
        //
        //     if {
        //         let filterable: Box<dyn NameFilters> = Box::new(&p);
        //         filterable.filter_names(id.clone().unwrap_or_default(), ns)
        //     } {
        //         let pod_ns = p.labels.get("skate.io/namespace").unwrap_or(&"default".to_string()).clone();
        //         return Some((NamespacedName::from(format!("{}.{}", deployment, pod_ns).as_str()), p));
        //     }
        //     None
        // }).collect();
        todo!()
    }

    fn list(&self, args: &GetObjectArgs, state: &ClusterState) -> Vec<DeploymentListItem> {
        let pods = state.nodes.iter().filter_map(|n| {
            let items: Vec<_> = n.host_info.clone()?.system_info?.pods.unwrap_or_default().into_iter().filter_map(|p| {
                let deployment = p.labels.get("skate.io/deployment").unwrap_or(&"".to_string()).clone();
                if deployment == "" {
                    return None;
                }

                if {
                    let filterable: Box<dyn NameFilters> = Box::new(&p);
                    filterable.filter_names(&args.id.clone().unwrap_or_default(), &args.namespace.clone().unwrap_or_default())
                } {
                    let pod_ns = p.labels.get("skate.io/namespace").unwrap_or(&"default".to_string()).clone();
                    return Some((NamespacedName::from(format!("{}.{}", deployment, pod_ns).as_str()), p));
                }
                None
            }).collect();
            match items.len() {
                0 => None,
                _ => Some(items)
            }
        }).flatten();

        let grouped = pods.fold(HashMap::<NamespacedName, Vec<PodmanPodInfo>>::new(), |mut acc, (depl, pod)| {
            acc.entry(depl).or_insert(vec![]).push(pod);
            acc
        });

        grouped.iter().map(|(name, pods)| {
            let health_pods = pods.iter().filter(|p| PodmanPodStatus::Running == p.status).collect_vec().len();
            let all_pods = pods.len();
            let created = pods.iter().fold(Local::now(), |acc, item| {
                if item.created < acc {
                    return item.created;
                }
                return acc;
            });

            let its_age = age(created);
            let healthy = format!("{}/{}", health_pods, all_pods);
            DeploymentListItem {
                namespace: name.namespace.clone(),
                name: name.name.clone(),
                ready: healthy,
                up_to_date: all_pods.to_string(),
                available: health_pods.to_string(),
                age: its_age,
            }
        }).collect()
    }

    // fn print(&self, items: Vec<(NamespacedName, PodmanPodInfo)>) {
    //     let pods = items.into_iter().fold(HashMap::<NamespacedName, Vec<PodmanPodInfo>>::new(), |mut acc, (depl, pod)| {
    //         acc.entry(depl).or_insert(vec![]).push(pod);
    //         acc
    //     });
    //
    //     let mut rows = vec!();
    //
    //     rows.push(["NAMESPACE", "NAME", "READY", "UP-TO-DATE", "AVAILABLE", "AGE"].map(|i| i.to_string()));
    //
    //     for (deployment, pods) in pods {
    //         let health_pods = pods.iter().filter(|p| PodmanPodStatus::Running == p.status).collect_vec().len();
    //         let all_pods = pods.len();
    //         let created = pods.iter().fold(Local::now(), |acc, item| {
    //             if item.created < acc {
    //                 return item.created;
    //             }
    //             return acc;
    //         });
    //
    //         let its_age = age(created);
    //         let healthy = format!("{}/{}", health_pods, all_pods);
    //         rows.push([deployment.namespace, deployment.name, healthy, all_pods.to_string(), health_pods.to_string(), its_age]);
    //     }
    //
    //     let mut table = Builder::from_iter(rows).build();
    //     table.with(Style::empty());
    //     println!("{}", table);
    // }
}
