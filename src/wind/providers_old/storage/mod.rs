use anyhow::Result;
use serde::{Deserialize, Serialize};

pub(crate) mod web_sys;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StorageConfig {
//   Local{
//     dir: String
//   },
//   ObjectStorage {
//     endpoint: String,
//     region: String,
//     bucket: String,
//     access_key: String,
//     secret_key: String,
//   },
    WebSys {
        prefix: String,
    }
}

pub trait Storage {

    async fn save(&self, name: String) -> Result<()>;
    
    async fn remove(&self, name: String) -> Result<()>;

    async fn exists(&self, name: String) -> Result<bool>;

}
