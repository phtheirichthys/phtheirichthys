use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use log::{info};
use serde::{Serialize, Deserialize};
use tsify_next::Tsify;
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
pub struct Race {
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
    pub(crate) restricted_zones: Vec<RestrictedZone>
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct RestrictedZone {
    name: String,
    color: Option<String>,
    vertices: Vec<Coords>,
    bbox: [f64; 4],
    #[serde(default)]
    to_avoids: Vec<(Coords, Coords, Coords)>
}

impl RestrictedZone {
    pub(crate) fn compute(&mut self) {
        let mut vertices = self.vertices.clone();
        while vertices.len() > 2 {
            self.to_avoids.push((vertices[0].clone(), vertices[1].clone(), vertices[2].clone()));
            vertices.remove(1);
        }
    }
    pub(crate) fn is_in(&self, point: &Coords) -> bool {
        let mut lon = point.lon;
        while lon > 180f64 {
            lon -= 360f64;
        }
        while lon < -180f64 {
            lon += 360f64;
        }

        if point.lat < self.bbox[0] || point.lat > self.bbox[2] || lon < self.bbox[1] || lon > self.bbox[2] {
            return false;
        }

        for t in self.to_avoids.iter() {
            let as_x = point.lat - t.0.lat;
            let as_y = point.lon - t.0.lon;

            let s_ab = (t.1.lat-t.0.lat)*as_y-(t.1.lon-t.0.lon)*as_x > 0.0;

            if ((t.2.lat-t.0.lat)*as_y-(t.2.lon-t.0.lon)*as_x > 0.0) == s_ab {
                continue
            }

            if ((t.2.lat-t.1.lat)*(point.lon-t.1.lon)-(t.2.lon-t.1.lon)*(point.lat-t.1.lat) > 0.0) != s_ab {
                continue
            }

            return true
        }

        false
    }
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

impl Limits {
    pub(crate) fn is_in(&self, point: &Coords) -> bool {

        let mut lon = point.lon;
        while lon > 180f64 {
            lon -= 360f64;
        }
        while lon < -180f64 {
            lon += 360f64;
        }

        if self.min_lat < point.lat && point.lat < self.max_lat {
            return false
        }

        if point.lat > 0.0 {
            if self.north.len() == 0 {
                return false
            }

            let i = ((lon + 180f64) / 5f64) as usize;
            let lat = (lon - self.north[i].lon)/(self.north[i+1].lon-self.north[i].lon)*(self.north[i+1].lat-self.north[i].lat) + self.north[i].lat;
            if point.lat >= lat {
                return true
            }
        } else {
            if self.south.len() == 0 {
                return false
            }

            let i = ((lon + 180f64) / 5f64) as usize;
            let lat = (lon-self.south[i].lon)/(self.south[i+1].lon-self.south[i].lon)*(self.south[i+1].lat-self.south[i].lat) + self.south[i].lat;
            if point.lat <= lat {
                return true
            }
        }

        false
    }
}

#[derive(Clone, Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "type")]
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
