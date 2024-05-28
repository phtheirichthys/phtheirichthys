use std::fmt::{Display, Formatter};
use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_humanize::HumanTime;
use serde::{Serialize, Serializer, Deserialize};
use crate::phtheirichthys::BoatOptions;
use crate::position::{Heading, Penalties, Point, Settings, Status};
use crate::wind::Wind;
use crate::{position, race::Race};
use crate::utils::{Speed};

// pub(crate) mod phtheirichthys;
pub(crate) mod echeneis;

#[async_trait]
pub(crate) trait Router {
  async fn route(&self, race: &Race, boat_options: BoatOptions, request: RouteRequest, timeout: Option<Duration>) -> Result<RouteResult>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RouteRequest {
  pub(crate) from: Point,
  pub(crate) start_time: DateTime<Utc>,
  pub(crate) boat_settings: Settings,
  pub(crate) status: Status,
  #[serde(skip, default = "default_steps")]
  pub(crate) steps: Vec<(Duration, Duration)>,
}

fn default_steps() -> Vec<(Duration, Duration)> {
  vec![
    (Duration::minutes(30), Duration::minutes(1)),
    (Duration::hours(1),    Duration::minutes(5)),
    (Duration::hours(3),    Duration::minutes(10)),
    (Duration::hours(12),   Duration::minutes(30)),
    (Duration::hours(24),   Duration::hours(1)),
    (Duration::hours(144),  Duration::hours(3)),
    (Duration::hours(9999), Duration::hours(6)),
  ]
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteInfos {
  pub(crate) start: DateTime<Utc>,
  duration: f64,
  success: bool,
  sails_duration: HashMap<usize, f64>,
  foil_duration: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteResult {
  pub(crate) infos: RouteInfos,
  pub(crate) way: Vec<Waypoint>,
  sections: Vec<IsochroneSection>,
  debug: Vec<IsochronePoint>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Waypoint {
  pub(crate) from: Point,
  #[serde(serialize_with = "duration_to_seconds")]
  pub(crate) duration: Duration,
  #[serde(serialize_with = "duration_to_seconds")]
  pub(crate) way_duration: Duration,
  pub(crate) boat_settings: Settings,
  pub(crate) status: WaypointStatus,
}

fn duration_to_seconds<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
  where S: Serializer {
  serializer.serialize_i64(duration.num_seconds())
}

impl Display for Waypoint {
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

#[derive(Clone, Debug, Serialize)]
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

impl Into<Status> for WaypointStatus {
  fn into(self) -> Status {
    Status {
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

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Penalty {
  #[serde(serialize_with = "duration_to_seconds")]
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

#[derive(Clone, Debug, Serialize)]
struct IsochroneSection {
  door: String,
  isochrones: Vec<Isochrone>
}

#[derive(Clone, Debug, Serialize)]
struct Isochrone {
  color: String,
  paths: Vec<Vec<IsochronePoint>>,
}

#[derive(Clone, Debug, Serialize)]
struct IsochronePoint {
  pub(crate) lat: f64,
  pub(crate) lon: f64,
  pub(crate) az: i32,
  pub(crate) previous: i32,
}