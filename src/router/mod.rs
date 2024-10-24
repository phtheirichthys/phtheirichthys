use std::fmt::{Display, Formatter};
use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_humanize::HumanTime;
use serde::{Serialize, Serializer, Deserialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;
use crate::phtheirichthys::BoatOptions;
use crate::position::{Heading, Penalties, Coords, BoatSettings, BoatStatus};
use crate::wind::Wind;
use crate::{position, race::Race};
use crate::utils::Speed;

// pub(crate) mod phtheirichthys;
pub(crate) mod echeneis;

#[async_trait]
pub(crate) trait Router {
  async fn route(&self, race: &Race, boat_options: BoatOptions, request: RouteRequest, timeout: Option<Duration>) -> Result<RouteResult>;
}

#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RouteRequest {
  pub from: Coords,
  #[tsify(type = "Date")]
  pub start_time: DateTime<Utc>,
  pub boat_settings: BoatSettings,
  pub status: BoatStatus,
  #[serde(skip, default = "default_steps")]
  pub steps: Vec<(Duration, Duration)>,
}

fn default_steps() -> Vec<(Duration, Duration)> {
  vec![
    (Duration::hours(1),    Duration::minutes(10)),
    (Duration::hours(6),    Duration::hours(1)),
    (Duration::hours(24),   Duration::minutes(3)),
    (Duration::hours(9999), Duration::hours(6)),
  ]
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct RouteInfos {
  pub(crate) start: DateTime<Utc>,
  duration: f64,
  success: bool,
  sails_duration: HashMap<usize, f64>,
  foil_duration: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct RouteResult {
  pub(crate) infos: RouteInfos,
  pub(crate) way: Vec<RouteWaypoint>,
  sections: Vec<IsochroneSection>,
  debug: Vec<IsochronePoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct RouteWaypoint {
  pub(crate) from: Coords,
  #[serde(serialize_with = "duration_to_seconds", deserialize_with = "seconds_to_duration")]
  pub(crate) duration: Duration,
  #[serde(serialize_with = "duration_to_seconds", deserialize_with = "seconds_to_duration")]
  pub(crate) way_duration: Duration,
  pub(crate) boat_settings: BoatSettings,
  pub(crate) status: WaypointStatus,
}

fn duration_to_seconds<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
  where S: Serializer {
  serializer.serialize_i64(duration.num_seconds())
}

fn seconds_to_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where D: serde::Deserializer<'de>
{
  let buf = i64::deserialize(deserializer)?;

  Ok(Duration::seconds(buf))
}

impl Display for RouteWaypoint {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self.boat_settings.heading {
      Heading::HEADING(heading) => {
        write!(f, "heading {} {} using {}", heading, HumanTime::from(self.duration), self.boat_settings.sail)
      }
      Heading::TWA(twa) => {
        write!(f, "regulated twa {} {} using {}", twa, HumanTime::from(self.duration), self.boat_settings.sail)
      }
    }
  }
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct WaypointStatus {
  pub(crate) boat_speed: Speed,
  pub(crate) wind: Wind,
  pub(crate) foil: u8,
  pub(crate) boost: u8,
  pub(crate) best_ratio: f64,
  pub(crate) ice: bool,
  pub(crate) change: bool,
  pub(crate) penalties: Vec<Penalty>,
  pub(crate) remaining_penalties: Vec<Penalty>,
  pub(crate) stamina: f64,
  pub(crate) remaining_stamina: f64,
}

impl Into<BoatStatus> for WaypointStatus {
  fn into(self) -> BoatStatus {
    BoatStatus {
      aground: false,
      boat_speed: self.boat_speed,
      wind: self.wind,
      foil: self.foil,
      boost: self.boost,
      best_ratio: self.best_ratio,
      ratio: 100,
      vmgs: None,
      penalties: Penalties::default(),
      stamina: 0.0,
    }
  }
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Penalty {
  #[serde(serialize_with = "duration_to_seconds", deserialize_with = "seconds_to_duration")]
  pub(crate) duration: Duration,
  pub(crate) ratio: f64,
  pub(crate) typ: u8,
}

impl From<position::Penalty> for Penalty {
  fn from(penalty: position::Penalty) -> Self {
    Self {
      duration: penalty.duration,
      ratio: penalty.ratio,
      typ: 0
    }
  }
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
struct IsochroneSection {
  door: String,
  isochrones: Vec<Isochrone>
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
struct Isochrone {
  color: String,
  paths: Vec<Vec<IsochronePoint>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
struct IsochronePoint {
  pub(crate) lat: f64,
  pub(crate) lon: f64,
  pub(crate) az: i32,
  pub(crate) previous: i32,
}
