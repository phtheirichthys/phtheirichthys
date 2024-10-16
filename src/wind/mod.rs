use std::{collections::BTreeMap, fmt::{Display, Formatter}};

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tsify_next::Tsify;

use crate::{position::Coords, utils::Speed};

pub mod providers;
mod stamp;

#[cfg(test)]
mod tests;

pub(crate) trait Provider {
    fn start(&self);

    fn status(&self) -> ProviderStatus;

    fn find(&self, m: &DateTime<Utc>) -> Box<dyn InstantWind + Send + Sync>;
}

#[derive(Serialize, Deserialize)]
pub struct ProviderStatus {
    pub current_ref_time: RefTime,
    pub last: Option<ForecastTime>,
    pub progress: u8,
    pub forecasts: BTreeMap<ForecastTime, Vec<RefTime>>,
}

type RefTime = DateTime<Utc>;

type ForecastTime = DateTime<Utc>;

pub(crate) trait InstantWind {
    fn interpolate(&self, point: &Coords) -> Wind;
}

pub(crate) fn vector_to_degrees(u: f64, v: f64) -> f64 {
    let velocity_dir = libm::atan2(u, v);
    let velocity_dir_to_degrees = velocity_dir.to_degrees() + 180.0;

    velocity_dir_to_degrees
}
  
#[derive(Clone, Debug, Default, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Wind {
    pub direction: f64,
    #[tsify(type = "number")]
    pub speed: Speed,
}

impl Display for Wind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}° {}kts", self.direction, self.speed.kts())
    }
}

impl Wind {
    pub(crate) fn gap(&self, other: &Self) -> u8 {
        let mut diff = (self.direction - other.direction).abs();
        if diff > 180.0 {
            diff = (diff - 360.0).abs()
        }

        (diff / 360.0 * 100.0) as u8
    }
}
