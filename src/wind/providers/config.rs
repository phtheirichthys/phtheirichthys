use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderConfig {
  Noaa(NoaaProviderConfig),
//   Meteofrance(MeteofranceProviderConfig),
//   Zezo(ZezoProviderConfig),
  Vr,
}

impl From<&str> for ProviderConfig {
  fn from(value: &str) -> Self {
    match value {
      "noaa" => Self::Noaa(NoaaProviderConfig { url: "https://winds2.phtheirichthys.fr".to_string() }),
      "vr" | _ => Self::Vr,
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoaaProviderConfig {
  pub(crate) url: String,
}
