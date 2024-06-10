use anyhow::{bail, Result};
use std::{collections::HashMap, f64::consts::PI, ops::Add, sync::{Arc, RwLock}};

use config::ProviderConfig;
use log::{debug, error, info};
use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, ImageData};

use crate::position::Coords;

pub(crate) mod config;
pub(crate) mod vr;

pub(crate) struct Providers {
    providers: Arc<RwLock<HashMap<String, Arc<Box<dyn LandsProvider + Sync + Send>>>>>,
}

impl Providers {
    pub(crate) fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new()))
        }
    }

    pub(crate) async fn init_provider(&self, config: &ProviderConfig) -> Result<()> {
        info!("Init provider");

        match config {
            ProviderConfig::Vr => {
                let providers = self.providers.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    match vr::VrLandProvider::new() {
                        Ok(vr) => {
                            let mut providers: std::sync::RwLockWriteGuard<HashMap<String, Arc<Box<dyn LandsProvider + Sync + Send>>>> = providers.write().unwrap();
                            providers.insert("vr".into(), Arc::new(vr));
                        },
                        Err(e) => {
                            error!("Failed starting vr land provider : {}", e);
                        }
                    }
                });
            }
        }

        Ok(())
    }

    pub(crate) fn draw(&self, provider: String, ctx: &CanvasRenderingContext2d, x: i64, y: i64, z: u32, width: usize, height: usize) -> Result<()> {
        debug!("Draw land {provider} ({x},{y},{z})");

        let providers: std::sync::RwLockReadGuard<HashMap<String, Arc<Box<dyn LandsProvider + Sync + Send>>>> = self.providers.read().unwrap();

        match providers.get(&provider) {
            Some(provider) => {
                debug!("Found provider");

                provider.draw(ctx, x, y, z, width, height)
            },
            None => {
                bail!("Provider not found")
            },
        }
    }

}

pub(crate) trait LandsProvider {
    fn is_land(&self, lat: f64, lon: f64) -> bool;

    fn is_next_land(&self, lat: f64, lon: f64) -> bool {
        for i in -1..2 {
            for j in -1..2 {
                let lat = lat + (i as f64) / (730.0 / 2.0);
                let lon = lon + (j as f64) / (730.0 / 2.0);

                if self.is_land(lat, lon) {
                    return true
                }
            }
        }

        return false
    }

    fn _cross_land(&self, from: &Coords, to: &Coords) -> bool {

        const STEP: i8 = 10;

        for i in 0..(STEP + 1) {
            let lat = from.lat + (i as f64) * (to.lat - from.lat) / (STEP as f64);
            let lon = from.lon + (i as f64) * (to.lon - from.lon) / (STEP as f64);
            if self.is_land(lat, lon) {
                return true;
            }
        }

        false
    }

    fn cross_next_land(&self, from: &Coords, to: &Coords) -> bool {

        let next = self.is_next_land(from.lat, from.lon);

        const STEP: i8 = 10;

        for i in 0..(STEP + 1) {
            let lat = from.lat + (i as f64) * (to.lat - from.lat) / (STEP as f64);
            let lon = from.lon + (i as f64) * (to.lon - from.lon) / (STEP as f64);
            if next && self.is_land(lat, lon) || !next && self.is_next_land(lat, lon) {
                return true;
            }
        }

        false
    }

    fn _best_to_leave(&self, from: &Coords) -> f64 {

        let deltas = [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0)];
        let headings = [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

        let distances = [
            [0, 1, 2, 3, 4, 3, 2, 1],
            [1, 0, 1, 2, 3, 4, 3, 2],
            [2, 1, 0, 1, 2, 3, 4, 3],
            [3, 2, 1, 0, 1, 2, 3, 4],
            [4, 3, 2, 1, 0, 1, 2, 3],
            [3, 4, 3, 2, 1, 0, 1, 2],
            [2, 3, 4, 3, 2, 1, 0, 1],
            [1, 2, 3, 4, 3, 2, 1, 0],
        ];

        let mut lands = [false;8];
        let mut scores = [0;8];

        for i in 0..8 {
            let lat = from.lat + deltas[i].0 * 0.7/730.0;
            let lon = from.lon + deltas[i].1 * 0.7/730.0;
            lands[i] = self.is_land(lat, lon);
        }

        debug!("lands : {:?}", lands);

        for i in 0..8 {
            scores[i] = distances[i].iter().enumerate()
                .filter(|(o, _)| lands[*o])
                .min_by(|(_, a), (_, b)| a.cmp(b))
                .map_or(0, |(_, d)| *d);
        }

        debug!("scores : {:?}", scores);

        headings.iter().enumerate().max_by(|(a, _), (b, _)| scores[*a].cmp(&scores[*b])).map(|(_, heading)| *heading).unwrap()
    }

    fn near_land(&self, lat: f64, lon: f64) -> bool {
        for i in -2..3 {
            for j in -2..3 {
                if self.is_land(lat + (i as f64) / 730.0, lon + (j as f64) / 730.0) {
                    return true
                }
            }
        }

        false
    }

    fn draw(&self, ctx: &CanvasRenderingContext2d, x: i64, y: i64, z: u32, width: usize, height: usize) -> Result<()> {
        let mut data = vec![0u8; width * height * 4];

        let bb = tile2boudingbox(x as f64, y as f64, z as f64);

        for i in 0..width {
            for j in 0..height {

			    let lat: f64 = bb.north + j as f64 * (bb.south-bb.north) / height as f64;
			    let lon = bb.west + i as f64 * (bb.east-bb.west) / width as f64;

                let (lat, lon) = to_lat_lon((x * width as i64 + i as i64) as f64, (y * height as i64 + j as i64) as f64, z as f64);

                if self.is_land(lat, lon) {
                    data[(j * width + i) * 4] = 0;
                    data[(j * width + i) * 4 + 1] = 0;
                    data[(j * width + i) * 4 + 2] = 0;
                    data[(j * width + i) * 4 + 3] = 255;
                }

                // if i == 0 || j == 0 || i == width - 1 || j == height - 1 {
                //     data[(j * width + i) * 4] = 0;
                //     data[(j * width + i) * 4 + 1] = 0;
                //     data[(j * width + i) * 4 + 2] = 255;
                //     data[(j * width + i) * 4 + 3] = 255;
                // }

                // if lat.abs() / 10.0 - ((lat.abs() as i64 / 10) as f64) < (bb.south-bb.north).abs() / height as f64 ||
                //     lon.abs() / 10.0 - ((lon.abs() as i64 / 10) as f64) < (bb.east-bb.west).abs() / width as f64
                // {
                //     data[(j * width + i) * 4] = 0;
                //     data[(j * width + i) * 4 + 1] = 255;
                //     data[(j * width + i) * 4 + 2] = 0;
                //     data[(j * width + i) * 4 + 3] = 255;
                // }
            }
        }
        let data = match ImageData::new_with_u8_clamped_array_and_sh(Clamped(&data), width as u32, height as u32) {
            Ok(data) => data,
            Err(e) => {
                bail!("Error creating image from data : {:?}", e);
            }
        };
        match ctx.put_image_data(&data, 0.0, 0.0) {
            Ok(_) => {},
            Err(e) => {
                bail!("Error creating image from canvas context : {:?}", e);
            }
        }
        Ok(())
    }
}

struct BoundingBox {
	north: f64,
	south: f64,
	east: f64,
	west: f64,
}

fn to_lat_lon(x: f64, y: f64, z: f64) -> (f64, f64) {
    let size = 256.0 * 2_f64.powf(z);
    let bc = size / 360.0;
    let cc = size / (2.0 * PI);
    let zc = size / 2.0;
    let g = (y - zc) / -cc;
    let lon = (x - zc) / bc;
    let lat = (2.0 * g.exp().atan() - 0.5 * PI).to_degrees();

    (lat, lon)
}

fn tile2boudingbox(x: f64, y: f64, z: f64) -> BoundingBox {
    let ll = (x * 256.0, (y + 1.0) * 256.0);
    let ur = ((x + 1.0) * 256.0, y * 256.0);

    let size = 256.0 * 2_f64.powf(z);
    let bc = size / 360.0;
    let cc = size / (2.0 * PI);
    let zc = size / 2.0;
    let g = (ll.1 - zc) / -cc;
    let west = (ll.0 - zc) / bc;
    let south = (2.0 * g.exp().atan() - 0.5 * PI).to_degrees();

    let g = (ur.1 - zc) / -cc;
    let east = (ur.0 - zc) / bc;
    let north = (2.0 * g.exp().atan() - 0.5 * PI).to_degrees();

    return BoundingBox{
		north,
        south,
        east,
        west,
	}
}

fn tile2boudingbox2(x: f64, y: f64, z: f64) -> BoundingBox {
    const R: f64 = 6378137.0;

    let scale = 0.5 / (PI * R);
    let (a, b, c, d) = (scale, 0.5, -1.0 * scale, 0.5);

    let scale = 256.0 * 2_f64.powf(z);

    let (north, west) = {
        let (x, y) = ((x / scale - b) / a, (y / scale - d) / c);
        ((2.0 * (y / R).exp().atan() - PI / 2.0) * d, x * d / R)
    };

    let (south, east) = {
        let (x, y) = (((x - 1.0) / scale - b) / a, ((y - 1.0) / scale - d) / c);
        ((2.0 * (y / R).exp().atan() - PI / 2.0).to_degrees(), (x / R).to_degrees())
    };
    
	return BoundingBox{
		north,
        south,
        east,
        west,
	}
}