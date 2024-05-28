use serde::{Deserialize, Serialize};

use super::storage::StorageConfig;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderConfig {
  Noaa(NoaaProviderConfig),
//   Meteofrance(MeteofranceProviderConfig),
//   Zezo(ZezoProviderConfig),
  Vr,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoaaProviderConfig {
  pub enabled: bool,
//   pub init: Option<DateTime<Utc>>,
  pub gribs: StorageConfig,
}
