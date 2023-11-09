use std::collections::{HashMap};
use std::env::consts::ARCH;
use sysinfo::{CpuRefreshKind, RefreshKind, System, SystemExt};
use std::error::Error;
use std::str::FromStr;
use chrono::{DateTime, Local};
use clap::{Args, Subcommand};
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use crate::skate::{Distribution, exec_cmd, Os, Platform};


#[derive(Debug, Args)]
pub struct SystemArgs {
    #[command(subcommand)]
    command: SystemCommands,
}


#[derive(Debug, Subcommand)]
pub enum SystemCommands {
    #[command(about = "report system information")]
    Info,
}

pub async fn system(args: SystemArgs) -> Result<(), Box<dyn Error>> {
    match args.command {
        SystemCommands::Info => info().await?
    }
    Ok(())
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub platform: Platform,
    pub total_memory_mib: u64,
    pub used_memory_mib: u64,
    pub total_swap_mib: u64,
    pub used_swap_mib: u64,
    pub num_cpus: usize,
    pub pods: Option<Vec<PodmanPodInfo>>,
}

// TODO - have more generic ObjectMeta type for explaining existing resources
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanPodInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created: DateTime<Local>,
    pub labels: HashMap<String, String>,
    pub containers: Vec<PodmanContainerInfo>,
}

impl PodmanPodInfo {
    pub fn namespace(&self) -> String {
        self.labels.get("skate.io/namespace").map(|ns| ns.clone()).unwrap_or("".to_string())
    }
    pub fn deployment(&self) -> String {
        self.labels.get("skate.io/deployment").map(|d| d.clone()).unwrap_or("".to_string())
    }
}


impl Into<Pod> for PodmanPodInfo {
    fn into(self) -> Pod {
        Pod {
            metadata: ObjectMeta {
                annotations: None,
                creation_timestamp: None,
                deletion_grace_period_seconds: None,
                deletion_timestamp: None,
                finalizers: None,
                generate_name: None,
                generation: None,
                labels: None,
                managed_fields: None,
                name: Some(self.name.clone()),
                namespace: Some(self.namespace()),
                owner_references: None,
                resource_version: None,
                self_link: None,
                uid: None,
            },
            spec: None,
            status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerInfo {
    pub id: String,
    pub names: String,
    pub status: String,
    pub restart_count: Option<usize>,
}

async fn info() -> Result<(), Box<dyn Error>> {
    let sys = System::new_with_specifics(RefreshKind::new()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory()
        .with_networks()
    );
    let os = Os::from_str(&(sys.name().ok_or("")?)).unwrap_or(Os::Unknown);

    let result = exec_cmd(
        "podman",
        &["pod", "ps", "--filter", "label=skate.io/namespace", "--format", "json"],
    )?;
    let podman_pod_info: Vec<PodmanPodInfo> = serde_json::from_str(&result)?;


    let info = SystemInfo {
        platform: Platform {
            arch: ARCH.to_string(),
            os,
            distribution: Distribution::Unknown, // TODO
        },
        total_memory_mib: sys.total_memory(),
        used_memory_mib: sys.used_memory(),
        total_swap_mib: sys.total_swap(),
        used_swap_mib: sys.used_swap(),
        num_cpus: sys.cpus().len(),
        pods: Some(podman_pod_info),
    };
    let json = serde_json::to_string(&info)?;
    println!("{}", json);

    Ok(())
}