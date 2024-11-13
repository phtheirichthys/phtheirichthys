use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::io::{BufReader, Cursor};
use std::ops::Add;
use std::sync::{Arc, Mutex};
use anyhow::{bail, Result};
use byteorder::ReadBytesExt;
use chrono::{DateTime, Duration, DurationRound, Utc};
use chrono::serde::ts_seconds;
use clokwerk::{AsyncScheduler, Job};
use futures_util::future::ready;
#[cfg(feature = "wasm")]
use gloo::timers::callback::Interval;
use log::{debug, error, info};
use reqwest::Url;
use serde::{Deserialize, Serialize};
#[cfg(feature = "tokio")]
use tokio::task::spawn_local;
use bytes::{Buf, Bytes};
use bytes::buf::Reader;
use grib::{Grib2, GribError, GridDefinitionTemplateValues, SeekableGrib2Reader};
use grib::codetables::Lookup;
use crate::wind::ProviderStatus;
use crate::{position::Coords, utils::Speed, wind::{vector_to_degrees, InstantWind, Provider, Wind}};
use crate::wind::providers::config::NoaaProviderConfig;

pub(crate) struct NoaaWindProvider {
    config: NoaaProviderConfig,
    forecasts: Arc<Mutex<Forecasts>>,
}

unsafe impl Send for NoaaWindProvider {}
unsafe impl Sync for NoaaWindProvider {}

impl Provider for NoaaWindProvider {
    fn start(&self) {
        info!("Start NoaaWindProvider");

        let forecasts = self.forecasts.clone();
        let config = self.config.clone();

        #[cfg(feature = "wasm")]
        {
            let interval = Interval::new(10 * 60 * 1_000, move || {
                let forecasts = forecasts.clone();
                let config = config.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    Self::init(forecasts, config).await;
                });
            });

            interval.forget();
            //
            // wasm_bindgen_futures::spawn_local(async move {
            //     IntervalStream::new(10*60*1_000).for_each(move |_| {
            //         async move {
            //             future().await;
            //         }
            //     }).await
            // });
        }

        #[cfg(not(feature = "wasm"))]
        {
            let mut scheduler = AsyncScheduler::new();

            scheduler.every(10.minutes()).plus(1.minutes())
                .run(move || {
                    let forecasts = forecasts.clone();
                    let config = config.clone();
                    async move {
                        Self::init(forecasts, config).await;
                    }
                });
        }
    }


    fn status(&self) -> ProviderStatus {
        let forecasts: std::sync::MutexGuard<Forecasts> = self.forecasts.lock().unwrap();

        ProviderStatus {
            current_ref_time: forecasts.current_ref_time,
            last: forecasts.forecasts.last().map(|last| last.forecast_time),
            progress: 100,
            forecasts: forecasts.forecasts.iter().map(|forecasts| {
                let refs = forecasts.references.iter().map(|r| r.ref_time).collect::<Vec<_>>();
                (forecasts.forecast_time, refs)
            }).collect(),
        }
    }

    fn find(&self, m: &chrono::prelude::DateTime<chrono::prelude::Utc>) -> Box<dyn InstantWind + Send + Sync> {
        let m = m.add(Duration::minutes(-1)).duration_trunc(Duration::minutes(10)).expect("datetime rounded");

        let forecasts = self.forecasts.lock().unwrap();

        let mut previous: Option<&Forecast> = None;
        for forecast in forecasts.forecasts.iter() {
            if forecast.forecast_time > m {
                match previous {
                    None => {
                        let w1: Forecast = forecast.clone();
                        return Box::new(NoaaInstantWind { w1, w2: None, h: 0.0 });
                    }
                    Some(previous_forecast) => {
                        let h = (m.clone() - previous_forecast.forecast_time).num_minutes();
                        let delta = (forecast.forecast_time.clone() - previous_forecast.forecast_time).num_minutes();
                        let w1: Forecast = previous_forecast.clone();
                        if h == 0 {
                            return Box::new(NoaaInstantWind { w1, w2: None, h: 0.0 });
                        }
                        let w2: Forecast = forecast.clone();
                        return Box::new(NoaaInstantWind { w1, w2: Some(w2), h: h as f64 / delta as f64 });
                    }
                }
            }

            previous = Some(forecast);
        }

        let previous_forecast = previous.unwrap();
        let w1: Forecast = previous_forecast.clone();

        Box::new(NoaaInstantWind { w1, w2: None, h: 0.0 })
    }
}

impl NoaaWindProvider {
    pub(crate) async fn new(config: NoaaProviderConfig) -> Result<Self> {
        debug!("Create NoaaWindProvider");

        let forecasts = match Self::load(&config).await {
            Ok(mut forecasts) => {
                for forecast in forecasts.forecasts.iter_mut() {
                    for r in forecast.references.iter_mut() {
                        match r.load(&config).await {
                            Ok(_) => {}
                            Err(e) => {
                                bail!("Error loading forecast data : {}", e);
                            }
                        }
                    }
                }

                Arc::new(Mutex::new(forecasts))
            }
            Err(e) => {
                bail!("Error loading winds forecasts : {}", e);
            }
        };

        Ok(Self {
            config,
            forecasts,
        })
    }

    async fn load(config: &NoaaProviderConfig) -> Result<Forecasts> {
        info!("Load Noaa Wind forecasts");

        let client = reqwest::Client::new();
        let url = Url::parse("http://127.0.0.1:8000")?.join("winds/api/v2/winds/noaa")?;

        let response = client.get(url.clone())
            .send()
            .await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let mut forecasts = response.json::<Forecasts>().await?;

                let refs = vec![
                    0, 3, 6, 9, 12,
                    24, 36, 48,
                    72, 96, 120, 144, 168
                ];

                // Filter forecasts to keep only every 3h for 12h, then every 12h for 48h, then every 24h for 7 days
                let now = Utc::now();
                forecasts.forecasts.retain(|forecast| {
                    let delta = forecast.forecast_time - Utc::now();

                    if refs.iter().any(|r| (delta.num_minutes() - r * 60).abs() < 180) {
                        info!("keep {}", forecast.forecast_time);
                        true
                    } else {
                        false
                    }
                });

                Ok(forecasts)
            }
            n => {
                bail!("Error {} loading winds forecasts ({}) : {}", n, url, response.text().await?)
            }
        }
    }

    async fn init(forecasts: Arc<Mutex<Forecasts>>, config: NoaaProviderConfig) {
        match Self::load(&config).await {
            Ok(mut refs) => {
                let mut errors = false;

                for forecast in refs.forecasts.iter_mut() {
                    for r in forecast.references.iter_mut() {
                        let found = {
                            let mut forecasts = forecasts.lock().unwrap();
                            let (data, found) = forecasts.move_data(&forecast.forecast_time, &r.ref_time);
                            r.data = data;
                            found
                        };
                        if !found {
                            match r.load(&config).await {
                                Ok(_) => {}
                                Err(e) => {
                                    errors = true;
                                    error!("Error loading forecast data : {}", e);
                                }
                            }
                        }
                    }
                }

                if !errors {
                    let mut forecasts = forecasts.lock().unwrap();
                    *forecasts = refs;
                }
            }
            Err(e) => {
                error!("Error loading winds forecasts : {}", e);
            }
        }
    }
}

pub(crate) struct NoaaInstantWind {
    w1: Forecast,
    w2: Option<Forecast>,
    h: f64,
}

impl Display for NoaaInstantWind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let w2 = match &self.w2 {
            Some(w2) => format!("[{}]", w2.forecast_time.to_string()),
            None => String::new()
        };

        write!(f, "[{}]-{:.2}-{}", self.w1.forecast_time.to_string(), self.h, w2)
    }
}

impl NoaaInstantWind {
    fn floor_mod(a: f64, n: f64) -> f64 {
        let a = a + n;
        a - n * (a / n).floor()
    }

    fn bilinear_interpolate(x: f64, y: f64, g00: (f64, f64), g10: (f64, f64), g01: (f64, f64), g11: (f64, f64)) -> (f64, f64) {
        let rx = 1.0 - x;
        let ry = 1.0 - y;

        let a = rx * ry;
        let b = x * ry;
        let c = rx * y;
        let d = x * y;

        let u = g00.0 * a + g10.0 * b + g01.0 * c + g11.0 * d;
        let v = g00.1 * a + g10.1 * b + g01.1 * c + g11.1 * d;

        (u, v)
    }

    fn interpolate_from_data(data: &Data, pos: &Coords) -> (f64, f64) {

        let (lat, lon) = (pos.lat, pos.lon);

        let i = Self::floor_mod(lon - data.first_point_lon, 360f64) / data.delta_lon;
        let j = (lat - data.first_point_lat).abs() / data.delta_lat;

        let fi = i as usize;
        let fj = j as usize;

        let fi1 = if fi + 1 == data.u.len() { 0 } else { fi + 1 };
        let fj1 = fj + 1;


        let u00 = data.u[fi][fj] as f64;
        let v00 = data.v[fi][fj] as f64;

        let u01 = data.u[fi1][fj] as f64;
        let v01 = data.v[fi1][fj] as f64;

        let u10 = data.u[fi][fj1] as f64;
        let v10 = data.v[fi][fj1] as f64;

        let u11 = data.u[fi1][fj1] as f64;
        let v11 = data.v[fi1][fj1] as f64;

        Self::bilinear_interpolate(j - fj as f64, i - fi as f64, (u00, v00), (u10, v10), (u01, v01), (u11, v11))
    }

    fn interpolate(reference: &Reference, pos: &Coords) -> (f64, f64) {
        let data = reference.data.lock().expect("Lock poisoned");

        if data.is_none() {
            panic!("forecast not loaded : {}/{}", reference.ref_time, reference.forecast_time);
        }

        let data = *data.as_ref().as_ref().expect("Forecasts are not loaded");

        Self::interpolate_from_data(data, pos)
    }

    fn mid_interpolate(old: &Reference, new: Option<&Reference>, pos: &Coords, h_ref: f64) -> (f64, f64) {
        match new {
            None => {
                Self::interpolate(old, pos)
            }
            Some(new) => {
                let h = {
                    let d = 145.0 / 60.0;
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

impl InstantWind for NoaaInstantWind {
    fn interpolate(&self, pos: &Coords) -> Wind {
        let (mut u, mut v) = Self::mid_interpolate(&self.w1.references.iter().last().unwrap(), None, pos, self.h);

        if let Some(w2) = &self.w2 {
            let (u2, v2) = Self::mid_interpolate(&w2.references[0], w2.references.get(1), pos, self.h);
            u = u2 * self.h + u * (1.0 - self.h);
            v = v2 * self.h + v * (1.0 - self.h);
        }

        let mut d = Speed::from_m_s((u * u + v * v).sqrt());

        if d < Speed::MIN {
            d = Speed::MIN;
        }

        Wind {
            direction: vector_to_degrees(u, v),
            speed: d,
        }
    }
}


#[derive(Deserialize)]
pub(crate) struct Forecasts {
    provider: String,
    provider_name: String,
    current_ref_time: DateTime<Utc>,
    last: Option<LastForecast>,
    progress: u8,
    forecasts: Vec<Forecast>,
}

#[derive(Deserialize, Debug)]
struct LastForecast {
    forecast_time: DateTime<Utc>,
    ref_time: DateTime<Utc>,
}

#[derive(Clone, Deserialize)]
struct Forecast {
    forecast_time: DateTime<Utc>,
    references: Vec<Reference>,
}

#[derive(Clone, Deserialize)]
struct Reference {
    forecast_time: DateTime<Utc>,
    ref_time: DateTime<Utc>,
    #[serde(skip)]
    data: Arc<Mutex<Option<Data>>>,
}

struct Data {
    first_point_lat: f64,
    first_point_lon: f64,
    delta_lat: f64,
    delta_lon: f64,
    u: Vec<Vec<f32>>,
    v: Vec<Vec<f32>>,
}

type Grib = Grib2<SeekableGrib2Reader<Cursor<Vec<u8>>>>;

impl Forecasts {
    fn move_data(&mut self, forecast_time: &DateTime<Utc>, ref_time: &DateTime<Utc>) -> (Arc<Mutex<Option<Data>>>, bool) {
        for r in self.forecasts.iter_mut() {
            if &r.forecast_time != forecast_time {
                continue;
            }
            for r in r.references.iter() {
                if &r.ref_time == ref_time {
                    return (r.data.clone(), true);
                }
            }
        }

        (Arc::new(Mutex::new(None)), false)
    }
}


impl Reference {
    async fn load(&self, config: &NoaaProviderConfig) -> Result<()> {
        {
            if self.data.lock().unwrap().is_some() {
                return Ok(());
            }
        }

        let url = Url::parse("http://127.0.0.1:8000")?.join(&format!("winds/api/v2/winds/noaa/{}/{}", self.ref_time.format("%Y%m%d%H"), self.forecast_time.format("%Y%m%d%H")))?;
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
        let grib: Grib = grib::from_reader(Cursor::new(bytes.to_vec()))?;

        let mut u = None;
        let mut v = None;
        let mut first_point_lat = 90000000f64;
        let mut first_point_lon = 0f64;
        let mut delta_lat = 250000f64;
        let mut delta_lon = 250000f64;
        let factor = 1000000f64;

        for (_index, submessage) in grib.iter() {
            let discipline = submessage.indicator().discipline;
            let category = submessage.prod_def().parameter_category().unwrap();
            let parameter = submessage.prod_def().parameter_number().unwrap();

            match GridDefinitionTemplateValues::try_from(submessage.grid_def())? {
                GridDefinitionTemplateValues::Template0(def) => {
                    let mut data = vec![vec![0f32; def.nj as usize + 1]; def.ni as usize];

                    delta_lat = ((def.last_point_lat - def.first_point_lat).abs() / (def.nj - 1) as i32) as f64 / factor;
                    delta_lon= ((def.last_point_lon - def.first_point_lon).abs() / (def.ni - 1) as i32) as f64 / factor;

                    first_point_lat = def.first_point_lat as f64 / factor;
                    first_point_lon = def.first_point_lon as f64 / factor;

                    let latlons = submessage.latlons()?;
                    let decoder = grib::Grib2SubmessageDecoder::from(submessage)?;

                    let values = decoder.dispatch()?;

                    for ((lat, lon), value) in latlons.zip(values) {

                        let (lat, lon) = ( lat as f64, lon as f64 );

                        let i = (Self::floor_mod(lon - first_point_lon, 360f64) / delta_lon) as usize;
                        let j = ((lat - first_point_lat).abs() / delta_lat) as usize;

                        data[i][j] = value;
                    }

                    if discipline == 0 && category == 2 && parameter == 2 {
                        u = Some(data);
                    } else if discipline == 0 && category == 2 && parameter == 3 {
                        v = Some(data);
                    }
                }
                _ => bail!("Invalid GridDefinitionTemplateValues"),
            }
        }

        match (u, v) {
            (Some(u), Some(v)) => {
                let mut d = self.data.lock().unwrap();
                *d = Some(Data { first_point_lat, first_point_lon, delta_lat, delta_lon, u, v });
            }
            _ => bail!("Invalid GridDefinitionTemplateValues"),
        }

        Ok(())
    }

    fn floor_mod(a: f64, n: f64) -> f64 {
        a - n * (a / n).floor()
    }
}