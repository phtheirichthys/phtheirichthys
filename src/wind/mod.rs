use std::{collections::BTreeMap, collections::HashMap, fmt::{Display, Formatter}};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tsify_next::Tsify;

use crate::{position::Coords, utils::{self, Speed}};

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

    fn draw(&self, x: i64, y: i64, z: u32, width: usize, height: usize, f: Box<dyn FnOnce(&Vec<u8>) -> Result<()> + 'static>) -> Result<()> {
        let colors = vec![
            ( 98f64, 113f64, 184f64),
            ( 61f64, 110f64, 163f64),
            ( 74f64, 148f64, 170f64),
            ( 74f64, 146f64, 148f64),
            ( 77f64, 142f64, 124f64),
            ( 76f64, 164f64,  76f64),
            (103f64, 164f64,  54f64),
            (162f64, 135f64,  64f64),
            (162f64, 109f64,  92f64),
            (141f64,  63f64,  92f64),
            (151f64,  75f64, 145f64),
            ( 95f64, 100f64, 160f64),
            ( 91f64, 136f64, 161f64),
        ];

        let speeds = vec!(Speed::from_kts(0.0), Speed::from_kts(2.5), Speed::from_kts(5.0), Speed::from_kts(7.5), Speed::from_kts(10.0), Speed::from_kts(15.0), Speed::from_kts(20.0), Speed::from_kts(25.0), Speed::from_kts(30.0), Speed::from_kts(35.0), Speed::from_kts(40.0), Speed::from_kts(50.0), Speed::from_kts(60.0));

        let mut data = vec![0u8; width * height * 4];

        for i in 0..width {
            for j in 0..height {

                let (lat, lon) = utils::to_lat_lon((x * width as i64 + i as i64) as f64, (y * height as i64 + j as i64) as f64, z as f64);

                let wind = self.interpolate(&Coords { lat, lon });

                let mut s = 0;
                for k in 0..speeds.len() {
                    s = k;
                    if speeds[k] >= wind.speed {
                        break
                    }
                }

                let mut h = 0f64;
                let mut s_1 = s;
                let s_2 = s;
                if speeds[s_2] > wind.speed && s > 0 {
                    s_1 = s - 1;
                    h = (wind.speed.kts() - speeds[s_1].kts()) / (speeds[s_2].kts() - speeds[s_1].kts());
                }

                data[(j * width + i) * 4] = (colors[s_1].0*(1.0-h) + colors[s_2].0*h) as u8;
                data[(j * width + i) * 4 + 1] = (colors[s_1].1*(1.0-h) + colors[s_2].1*h) as u8;
                data[(j * width + i) * 4 + 2] = (colors[s_1].2*(1.0-h) + colors[s_2].2*h) as u8;
                data[(j * width + i) * 4 + 3] = 255;
            }
        }

        f(&data)
    }
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
