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
use crate::utils::Distance;

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
pub(crate) struct Race {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) leg: u8,
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
    pub(crate) buoys: Vec<Buoy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ice_limits: Option<Limits>,
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
pub(crate) enum Buoy {
    Zone(Zone),
    Door(Door),
    Waypoint(Waypoint),
}

impl Buoy {
    pub(crate) fn is_validated(&self) -> bool {
        match self {
            Buoy::Zone(circle) => circle.validated,
            Buoy::Door(door) => door.validated,
            Buoy::Waypoint(waypoint) => waypoint.validated,
        }
    }

    fn validate(&mut self) {
        match self {
            Buoy::Zone(circle) => circle.validated = true,
            Buoy::Door(door) => door.validated = true,
            Buoy::Waypoint(waypoint) => waypoint.validated = true,
        } 
    }
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Door {
    pub(crate) name: String,
    pub(crate) port: Coords,
    pub(crate) starboard: Coords,
    pub(crate) departure: Coords,
    pub(crate) destination: Coords,
    pub(crate) to_avoid: Vec<(Coords, Coords, Coords)>,
    pub(crate) validated: bool,
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Zone {
    pub(crate) name: String,
    pub(crate) destination: Coords,
    #[tsify(type = "number")]
    pub(crate) radius: Distance,
    pub(crate) to_avoid: Vec<(Coords, Coords, Coords)>,
    pub(crate) validated: bool,
}

impl Zone {
    pub(crate) fn is_in(&self, pos: &Coords) -> bool {
        Spherical{}.distance_to(&self.destination, pos) <= self.radius
    }
}


#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Waypoint {
    pub(crate) name: String,
    pub(crate) destination: Coords,
    pub(crate) to_avoid: Vec<(Coords, Coords, Coords)>,
    pub(crate) validated: bool,
}

impl Race {
    pub(crate) fn next_waypoint(&self) -> Option<Buoy> {

        self.buoys.iter().filter(|w| !w.is_validated()).collect::<Vec<_>>().first().map(|w| w.clone().to_owned())
    }

    pub(crate) fn validate_next_waypoint(&mut self) {

        info!("Validate next waypoint");
        self.buoys.iter_mut().filter(|w| !w.is_validated()).collect::<Vec<&mut Buoy>>().first_mut().map(|w| w.validate());
    }
}
