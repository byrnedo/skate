use std::error::Error;
use std::fs::{create_dir_all, DirEntry};
use std::io::Write;
use std::path::{Path, PathBuf};
use anyhow::anyhow;
use chrono::{DateTime, Local};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::{Metadata};
use k8s_openapi::api::batch::v1::CronJob;
use k8s_openapi::api::core::v1::{Secret, Service};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use tabled::Tabled;
use crate::errors::SkateError;
use crate::spec::cert::ClusterIssuer;
use crate::util::{metadata_name, NamespacedName};

// all dirs/files live under /var/lib/skate/store
// One directory for
// - ingress
// - cron
// directory structure example is
// /var/lib/skate/store/ingress/ingress-name.namespace/80.conf
// /var/lib/skate/store/ingress/ingress-name.namespace/443.conf
#[derive(Clone)]
pub struct FileStore {
    base_path: String,
}

#[derive(Tabled, Debug, Clone, Deserialize, Serialize)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ObjectListItem {
    pub name: NamespacedName,
    pub manifest_hash: String,
    #[tabled(skip)]
    pub manifest: Option<Value>,
    pub created_at: DateTime<Local>,
    pub path: String,
}

impl ObjectListItem {
    fn from_k8s_resource(res:  &(impl Metadata<Ty=ObjectMeta> + Serialize), path: Option<&str>) -> Self {

        let obj = ObjectListItem{
            name: metadata_name(res),
            manifest_hash: res.metadata().labels.as_ref().and_then(|l| l.get("skate.io/hash")).cloned().unwrap_or("".to_string()),
            manifest: Some(serde_yaml::to_value(res).expect("failed to serialize kubernetes object")),
            created_at: Local::now(),
            path: path.unwrap_or_default().to_string(),
        };
        obj
    }
}

impl TryFrom<&str> for ObjectListItem {
    type Error = Box<dyn Error>;

    fn try_from(dir: &str) -> Result<Self, Self::Error> {

        let file_name = Path::new(dir).file_name().ok_or(anyhow!("failed to get file name"))?;

        let ns_name = NamespacedName::from(file_name.to_str().unwrap());


        let hash_file_name = format!("{}/hash", dir);

        let hash = match std::fs::read_to_string(&hash_file_name) {
            Err(_) => {
                eprintln!("WARNING: failed to read hash file {}", &hash_file_name);
                "".to_string()
            }
            Ok(result) => result
        };

        let manifest_file_name = format!("{}/manifest.yaml", dir);
        let manifest: Option<Value> = match std::fs::read_to_string(&manifest_file_name) {
            Err(e) => {
                eprintln!("WARNING: failed to read manifest file {}: {}", &manifest_file_name, e);
                None
            }
            Ok(result) => Some(serde_yaml::from_str(&result).unwrap())
        };

        let metadata = std::fs::metadata(dir).map_err(|e| anyhow!(e).context(format!("failed to get metadata for {}", dir)))?;

        let created_at = metadata.created()?;
        Ok(ObjectListItem {
            name: ns_name,
            manifest_hash: hash,
            manifest,
            created_at: DateTime::from(created_at),
            path: dir.to_string(),
        })
    }
}

impl TryFrom<DirEntry> for ObjectListItem {
    type Error = Box<dyn Error>;


    fn try_from(dir_entry: DirEntry) -> Result<Self, Self::Error> {
        let path = dir_entry.path();



        Self::try_from(path.to_str().ok_or(anyhow!("failed to convert file name to string"))?)
    }
}

impl From<&Ingress> for ObjectListItem {
    fn from(res: &Ingress) -> Self {
        Self::from_k8s_resource(res, None)
    }
}

impl From<&CronJob> for ObjectListItem {
    fn from(res: &CronJob) -> Self {
        Self::from_k8s_resource(res, None)
    }
}

impl From<&Service> for ObjectListItem {
    fn from(res: &Service) -> Self {
        Self::from_k8s_resource(res, None)
    }
}


impl From<&Secret> for ObjectListItem {
    fn from(res: &Secret) -> Self {
        Self::from_k8s_resource(res, None)
    }
}

impl From<&ClusterIssuer> for ObjectListItem {
    fn from(res: &ClusterIssuer) -> Self {
        Self::from_k8s_resource(res, None)
    }
}

impl FileStore {
    pub fn new() -> Self {
        FileStore {
            base_path: "/var/lib/skate/store".to_string()
        }
    }

    fn get_path(&self, parts: &[&str]) -> String {
        let mut path = PathBuf::from(self.base_path.clone());
        path.extend(parts);
        path.to_string_lossy().to_string()
    }

    // will clobber
    pub fn write_file(&self, object_type: &str, object_name: &str, file_name: &str, file_contents: &[u8]) -> Result<String,SkateError> {
        let dir = self.get_path(&[object_type, object_name]);
        create_dir_all(&dir).map_err(|e| anyhow!(e).context(format!("failed to create directory {}", dir)))?;
        let file_path = format!("{}/{}/{}/{}", self.base_path, object_type, object_name, file_name);

        let file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&file_path);
        match file.map_err(|e| anyhow!(e).context(format!("failed to create file {}", file_path))) {
            Err(e) => Err(e.into()),
            Ok(mut file) => Ok(file.write_all(file_contents).map(|_| file_path)?)
        }
    }

    pub fn remove_file(&self, object_type: &str, object_name: &str, file_name: &str) -> Result<(), Box<dyn Error>> {
        let file_path = self.get_path(&[object_type, object_name, file_name]);
        let result = std::fs::remove_file(&file_path).map_err(|e| anyhow!(e).context(format!("failed to remove file {}", file_path)));
        if result.is_err() {
            return Err(result.err().unwrap().into());
        }
        Ok(())
    }

    pub fn exists_file(&self, object_type: &str, object_name: &str, file_name: &str) -> bool {
        let file_path = self.get_path(&[object_type, object_name, file_name]);
        std::path::Path::new(&file_path).exists()
    }

    // returns true if the object was removed, false if it didn't exist
    pub fn remove_object(&self, object_type: &str, object_name: &str) -> Result<bool, Box<dyn Error>> {
        let dir = self.get_path(&[object_type, object_name]);
        match std::fs::remove_dir_all(&dir) {
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => Ok(false),
                _ => Err(anyhow!(err).context(format!("failed to remove directory {}", dir)).into())
            }
            Ok(_) => Ok(true)
        }
    }

    pub fn get_object(&self, object_type: &str, object_name: &str) -> Result<ObjectListItem, Box<dyn Error>> {
        let dir = self.get_path(&[object_type, object_name]);

        let obj = ObjectListItem::try_from(dir.as_str())?;
        Ok(obj)
    }


    pub fn list_objects(&self, object_type: &str) -> Result<Vec<ObjectListItem>, Box<dyn Error>> {
        let dir = self.get_path(&[object_type]);
        let entries = match std::fs::read_dir(&dir) {
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                _ => return Err(anyhow!(e).context(format!("failed to read directory {}", dir)).into())
            },
            Ok(result) => result
        };

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| anyhow!(e).context("failed to read entry"))?;
            let obj = ObjectListItem::try_from(entry)?;
            result.push(obj);
        }
        Ok(result)
    }
}
