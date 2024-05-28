use chrono::{DateTime, Utc};
use log::info;
use serde::{Serialize, Deserialize};
use crate::algorithm::Algorithm;
use crate::algorithm::spherical::Spherical;
use crate::position::Point;

#[derive(Clone, Deserialize, Serialize, Debug)]
pub(crate) struct Race {
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
    pub(crate) start_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) end_time: Option<DateTime<Utc>>,
    pub(crate) start: Point,
    pub(crate) waypoints: Vec<Waypoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ice_limits: Option<Limits>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub(crate) struct RaceId {
    pub(crate) id: u16,
    pub(crate) leg: Option<u8>,
}


#[derive(Clone, Deserialize, Serialize, Debug)]
pub(crate) struct Limits {
    pub(crate) north: Vec<Point>,
    pub(crate) south: Vec<Point>,
    #[serde(rename = "maxLat")]
    pub(crate) max_lat: f64,
    #[serde(rename = "minLat")]
    pub(crate) min_lat: f64,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub(crate) struct Waypoint {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) radius: Option<u8>,
    pub(crate) latlons: Vec<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) departure: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) destination: Option<Point>,
    #[serde(rename = "toAvoid", skip_serializing_if = "Option::is_none")]
    pub(crate) to_avoid: Option<Vec<([f64;2], [f64;2], [f64;2])>>,
    #[serde(default)]
    pub(crate) validated: bool,
}

impl Waypoint {
    pub(crate) fn crossed(&self, from: &Point, to: &Point, heading: f64) -> bool {
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
