use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use log::info;
use serde::{Serialize, Deserialize};
use tsify::Tsify;
use crate::algorithm::Algorithm;
use crate::algorithm::spherical::Spherical;
use crate::position::Coords;

pub(crate) type Races = Arc<RwLock<HashMap<String, Race>>>;

pub(crate) trait RacesSpec {
    fn new() -> Self;

    fn list(&self) -> Vec<Race>;

    fn get(&self, name: &String) -> Result<Race>;

    fn set(&self, name: String, race: Race);
}

impl RacesSpec for Races {
    fn new() -> Self {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn list(&self) -> Vec<Race> {
        let races = self.read().unwrap();
        races.iter().map(|(_, r)| r.clone()).collect::<Vec<_>>()
    }

    fn get(&self, name: &String) -> Result<Race> {
        let races = self.read().unwrap();
        match races.get(name) {
            Some(race) => Ok(race.clone()),
            None => bail!("Race {name} not found"),
        }
    }
    
    fn set(&self, name: String, race: Race) {
        let mut races = self.write().unwrap();
        races.insert(name, race);
    }

    
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Race {
    pub(crate) race_id: RaceId,
    #[serde(default)]
    pub(crate) archived: bool,
    pub(crate) name: String,
    #[serde(rename = "shortName", skip_serializing_if = "Option::is_none")]
    pub(crate) short_name: Option<String>,
    pub(crate) boat: String,
    #[serde(default)]
    pub(crate) stamina: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(type = "Date")]
    pub(crate) start_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(type = "Date")]
    pub(crate) end_time: Option<DateTime<Utc>>,
    pub(crate) start: Coords,
    pub(crate) waypoints: Vec<Waypoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ice_limits: Option<Limits>,
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct RaceId {
    pub(crate) id: u16,
    pub(crate) leg: Option<u8>,
}


#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Limits {
    pub(crate) north: Vec<Coords>,
    pub(crate) south: Vec<Coords>,
    #[serde(rename = "maxLat")]
    pub(crate) max_lat: f64,
    #[serde(rename = "minLat")]
    pub(crate) min_lat: f64,
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Waypoint {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) radius: Option<u8>,
    pub(crate) latlons: Vec<Coords>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) departure: Option<Coords>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination: Option<Coords>,
    #[serde(rename = "toAvoid", skip_serializing_if = "Option::is_none")]
    pub(crate) to_avoid: Option<Vec<([f64;2], [f64;2], [f64;2])>>,
    #[serde(default)]
    pub(crate) validated: bool,
}

impl Waypoint {
    pub(crate) fn crossed(&self, from: &Coords, to: &Coords, heading: f64) -> bool {
        let algorithm = Spherical{};

        if self.latlons.len() > 1 {
            let babord = &self.latlons[0];
            let tribord = &self.latlons[1];


            let t = algorithm.heading_to(babord, tribord);
            let a = algorithm.heading_to(from, babord);

            let alpha = 180.0 + a - t;

            let b = algorithm.heading_to(from, tribord);
            let beta = b - t;

            if b < t
                && (a < b && heading > a && heading < b
                || a > b && (heading > a || heading < b)) {

                let a2 = algorithm.heading_to(to, babord);
                let mut alpha2 = 180.0 + a2 - t;
                if a2 > 180.0 {
                    alpha2 = alpha2 - 360.0
                }
                let b2 = algorithm.heading_to(to, tribord);
                let beta2 = b2 - t;

                return alpha*alpha2 < 0.0 && beta*beta2 < 0.0;
            }

        }

        false
    }
}

impl Race {
    pub(crate) fn next_waypoint(&self) -> Option<&Waypoint> {

        self.waypoints.iter().filter(|w| !w.validated).collect::<Vec<&Waypoint>>().first().map(|w| w.clone())
    }

    pub(crate) fn validate_next_waypoint(&mut self) {

        info!("Validate next waypoint");
        self.waypoints.iter_mut().filter(|w| !w.validated).collect::<Vec<&mut Waypoint>>().first_mut().map(|w| w.validated = true);
    }
}
