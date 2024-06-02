use std::collections::BTreeMap;
use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter};
use std::io::Cursor;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use anyhow::{bail, Result};
use byteorder::ReadBytesExt;
use chrono::{DateTime, Duration, DurationRound, Utc};
use chrono::serde::ts_seconds;
use gloo::timers::callback::Interval;
use log::{debug, error};
use reqwest::Url;
use serde::Deserialize;

use crate::wind::{ForecastTime, ProviderStatus, RefTime};
use crate::{position::Coords, utils::Speed, wind::{vector_to_degrees, InstantWind, Provider, Wind}};

#[derive(Debug)]
pub(crate) struct VrWindProvider {
    references: Arc<Mutex<References>>,
}

unsafe impl Send for VrWindProvider {}
unsafe impl Sync for VrWindProvider {}

impl Provider for VrWindProvider {
    fn start(&self) {
        debug!("Start vr VrWindProvider");

        let references = self.references.clone();

        let interval = Interval::new(10*60*1_000, move || {
            let references = references.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match Self::load().await {
                    Ok(mut refs) => {
                        let mut errors = false;

                        for reference in refs.references.iter_mut() {
                            for r in reference.iter_mut() {
                                let found = {
                                    let mut references = references.lock().unwrap();
                                    let (data, found) = references.move_data(&r.reference);
                                    r.data = data;
                                    found
                                };
                                if !found {
                                    match r.load().await {
                                        Ok(_) => {}
                                        Err(e) => {
                                            errors = true;
                                            error!("Error loading reference data : {}", e);
                                        }
                                    }
                                }
                            }
                        }

                        if !errors {
                            let mut references = references.lock().unwrap();
                            *references = refs;
                        }
                    },
                    Err(e) => {
                        error!("Error loading winds references : {}", e);
                    }
                }
            });
        });

        interval.forget();

    }

    fn status(&self) -> ProviderStatus {
        let references: std::sync::MutexGuard<References> = self.references.lock().unwrap();

        ProviderStatus {
            current_ref_time: references.start,
            last: references.references.last().map(|last| last[0].valid),
            progress: 100,
            forecasts: references.references.iter().map(|references| {
                let refs = references.iter().map(|r| r.valid - Duration::hours(r.delta_ref as i64)).collect::<Vec<_>>();
                (references[0].valid, refs)
            }).collect(),
        }
    }

    fn find(&self, m: &chrono::prelude::DateTime<chrono::prelude::Utc>) -> Box<dyn InstantWind> {
        let m = m.add(Duration::minutes(-1)).duration_trunc(Duration::minutes(10)).expect("datetime rounded");

        let references = self.references.lock().unwrap();

        let mut previous: Option<&Vec<Reference>> = None;
        for refs in references.references.iter() {
            let reference = &refs[0];
            if reference.valid > m {
                match previous {
                    None => {
                        let w1: Vec<Reference> = refs.iter().map_while(|s| {
                            Some(s.clone())
                        }).collect();
                        return Box::new(VrInstantWind { w1, w2: None, h: 0.0 });
                    }
                    Some(previous_refs) => {
                        let previous_ref = &previous_refs[0];
                        let h = (m.clone() - previous_ref.valid).num_minutes();
                        let delta = (reference.valid.clone() - previous_ref.valid.clone()).num_minutes();
                        let w1: Vec<Reference> = previous_refs.iter().map_while(|s| {
                            Some(s.clone())
                        }).collect();
                        if h == 0 {
                            return Box::new(VrInstantWind { w1, w2: None, h: 0.0 });
                        }
                        let w2: Vec<Reference> = refs.iter().map_while(|s| {
                            Some(s.clone())
                        }).collect();
                        return Box::new(VrInstantWind { w1, w2: Some(w2), h: h as f64 / delta as f64 });
                    }
                }
            }

            previous = Some(refs);
        }

        let previous_refs = previous.unwrap();
        let w1: Vec<Reference> = previous_refs.iter().map_while(|s| {
            Some(s.clone())
        }).collect();

        Box::new(VrInstantWind { w1, w2: None, h: 0.0 })
    }
}

impl VrWindProvider {

    pub(crate) async fn new() -> Result<Self> {
        debug!("Create VrWindProvider");

        let references = match Self::load().await {
            Ok(mut references) => {
                for reference in references.references.iter_mut() {
                    for r in reference {
                        match r.load().await {
                            Ok(_) => {}
                            Err(e) => {
                                bail!("Error loading reference data : {}", e);
                            }
                        }
                    }
                }

                Arc::new(Mutex::new(references))
            },
            Err(e) => {
                bail!("Error loading winds references : {}", e);
            }
        };

        Ok(Self {
            references,
        })
    }

    async fn load() -> Result<References> {
        debug!("Load Vr Wind References");

        let client = reqwest::Client::new();
        let url = Url::parse("https://static.virtualregatta.com")?.join("winds/live/references.json")?;

        let response = client.get(url.clone())
            .send()
            .await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let references = response.json::<References>().await?;

                Ok(references)
            }
            n => {
                bail!("Error {} loading winds references ({}) : {}", n, url, response.text().await?)
            }
        }
    }

}

#[derive(Debug)]
pub(crate) struct VrInstantWind {
    w1: Vec<Reference>,
    w2: Option<Vec<Reference>>,
    h: f64,
}

impl Display for VrInstantWind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let w2 = match &self.w2 {
            Some(w2) => format!("[{}]", w2.iter().map(|w| w.reference.clone()).collect::<Vec<String>>().join(",")),
            None => String::new()
        };

        write!(f, "[{}]-{:.2}-{}", self.w1.iter().map(|w| w.reference.clone()).collect::<Vec<String>>().join(","), self.h, w2)
    }
}

impl VrInstantWind {
    fn floor_mod(a: f64, n: f64) -> f64 {
        return a - n * (a/n).floor()
    }

    fn bilinear_interpolate(x: f64, y: f64, g00: (f64, f64), g10: (f64, f64), g01: (f64, f64), g11: (f64, f64)) -> (f64, f64) {
        let rx = 1.0 - x;
        let ry = 1.0 - y;

        let a = rx * ry;
        let b = x * ry;
        let c = rx * y;
        let d = x * y;

        let u = g00.0*a + g10.0*b + g01.0*c + g11.0*d;
        let v = g00.1*a + g10.1*b + g01.1*c + g11.1*d;

        (u, v)
    }

    fn interpolate_from_data(data: &Box<[[(f64,f64);360];181]>, pos: &Coords) -> (f64, f64) {

        let lat_0 = -90.0;
        let lon_0 = -180.0;

        let i = (pos.lat - lat_0).abs();
        let j = Self::floor_mod(pos.lon - lon_0, 360.0);

        let fi = i as usize;
        let fj = j as usize;

        let fi1 = (fi + 1).min(180);
        let fj1 = if fj + 1 == 360 { 0 } else { fj + 1 };

        let u00 = data[fi][fj].0;
        let v00 = data[fi][fj].1;

        let u01 = data[fi1][fj].0;
        let v01 = data[fi1][fj].1;

        let u10 = data[fi][fj1].0;
        let v10 = data[fi][fj1].1;

        let u11 = data[fi1][fj1].0;
        let v11 = data[fi1][fj1].1;

        return Self::bilinear_interpolate(j - fj as f64, i - fi as f64, (u00, v00), (u10, v10), (u01, v01), (u11, v11))
    }

    fn interpolate(reference: &Reference, pos: &Coords) -> (f64, f64) {

        let data = reference.data.lock().unwrap();

        if data.is_none() {
            panic!("reference not loaded : {:?}", reference);
        }

        let data = *data.as_ref().as_ref().unwrap();

        Self::interpolate_from_data(data, pos)
    }

    fn mid_interpolate(old: &Reference, new: Option<&Reference>, pos: &Coords, h_ref: f64) -> (f64, f64) {

        match new {
            None => {
                Self::interpolate(old, pos)
            }
            Some(new) => {
                let h = {
                    let d = (new.valid.timestamp() - new.avail.timestamp()) as f64 / (60.0 * 60.0);
                    (3.0 * h_ref - (3.0 - d)) / d
                };

                let (u1, v1) = Self::interpolate(old, pos);
                let (u2, v2) = Self::interpolate(new, pos);

                let u = u2 * h + u1 * (1.0 - h);
                let v = v2 * h + v1 * (1.0 - h);

                (u, v)
            }
        }
    }
}

impl InstantWind for VrInstantWind {
    fn interpolate(&self, pos: &Coords) -> Wind {
        let (mut u, mut v) = Self::mid_interpolate(&self.w1.iter().last().unwrap(), None, pos, self.h);

        if let Some(w2) = &self.w2 {
            let (u2, v2) = Self::mid_interpolate(&w2[0], w2.get(1), pos, self.h);
            u = u2 * self.h + u * (1.0 - self.h);
            v = v2 * self.h + v * (1.0 - self.h);
        }

        let mut d = Speed::from_km_h((u*u + v*v).sqrt());

        if d < Speed::MIN {
            d = Speed::MIN;
        }

        Wind {
            direction: vector_to_degrees(u, v),
            speed: d
        }
    }
}

#[derive(Debug, Deserialize)]
struct References {
    #[serde(rename="export_ts", with = "ts_seconds")]
    export: DateTime<Utc>,
    #[serde(rename="publish_ts", with = "ts_seconds")]
    publish: DateTime<Utc>,
    #[serde(rename="start_ts", with = "ts_seconds")]
    start: DateTime<Utc>,
    references: Vec<Vec<Reference>>,
}

impl References {
    fn move_data(&mut self, reference: &String) -> (Arc<Mutex<Option<Box<[[(f64,f64);360];181]>>>>, bool) {
        for r in self.references.iter_mut() {
            for r in r.iter() {
                if &r.reference == reference {
                    return (r.data.clone(), true);
                }
            }
        }

        (Arc::new(Mutex::new(None)), false)
    }
}


#[derive(Clone, Deserialize)]
struct Reference {
    reference: String,
    #[serde(rename="valid_ts", with = "ts_seconds")]
    valid: DateTime<Utc>,
    delta_ref: u8,
    delta: u8,
    #[serde(rename="avail_ts", with = "ts_seconds")]
    avail: DateTime<Utc>,
    rel_path: String,
    #[serde(skip)]
    data: Arc<Mutex<Option<Box<[[(f64,f64);360];181]>>>>,
}

impl Debug for Reference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reference")
            .field("reference", &self.reference)
            .field("valid", &self.valid)
            .field("delta_ref", &self.delta_ref)
            .field("delta", &self.delta)
            .field("avail", &self.avail)
            .field("rel_path", &self.rel_path)
            .finish()
    }
}

impl Reference {
     async fn load(&self) -> Result<()> {
        debug!("Load reference : {:?}", self);

        {
            if self.data.lock().unwrap().is_some() {
                return Ok(())
            }
        }

        let lat_0: i32 = -90;
        let lon_0 = -180;

        let url = Url::parse("https://static.virtualregatta.com")?.join(&format!("winds/{}", &self.rel_path))?;
        let client = reqwest::Client::new();

        debug!("Download from url {}", url);

        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(e) => {
                bail!("Error downloading file : {}", e);
            }
        };

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                bail!("Error getting content : {}", e);
            }
        };

        let mut bytes = Cursor::new(bytes);

        let mut buffer: Box<[[(f64, f64); 360]; 181]> = vec![[(0.0,0.0);360];181].try_into().unwrap();

        for lat in (-90..=90_i32).rev() {
            for lon in -180..180_i32 {
                let byte = bytes.read_i8()? as f64;
                let u = byte.signum() * (byte / 8.0).powi(2);
                let byte = bytes.read_i8()? as f64;
                let v = byte.signum() * (byte / 8.0).powi(2);

                buffer[(lat - lat_0) as usize][(lon - lon_0) as usize] = (u, v);
            }
        }

        let mut data = self.data.lock().unwrap();
        *data = Some(buffer);

        Ok(())
    }
}