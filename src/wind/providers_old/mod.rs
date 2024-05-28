pub mod config;
pub(crate) mod noaa;
pub mod storage;

use std::{collections::{BTreeMap, HashMap}, sync::{Arc, Mutex, RwLock}};

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

use crate::{debug, error, info};

use self::{config::ProviderConfig, noaa::Noaa, storage::Storage};

use super::stamp::{ForecastTime, ForecastTimeSpec, RefTime, Stamp, StampId};

pub type Providers = Arc<RwLock<HashMap<String, Box<dyn Provider + Send + Sync>>>>;

impl ProvidersSpec for Providers {}

pub trait ProvidersSpec {
    fn new() -> Providers {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn init_provider(&self, config: &ProviderConfig) {
        info!("Init provider");

        match config {
            ProviderConfig::Noaa(config) => {
                let noaa = Noaa::from_config(config);
                // let winds = noaa.load(true, false).await?;
                //noaa.init().await;
                wasm_bindgen_futures::spawn_local(async move {
                  noaa.start().await;
                });        
            },
        }
    }
}

pub trait Provider {
    fn id(&self) -> String;

    fn status(&self) -> Arc<RwLock<ProviderStatus>>;

    fn current_ref_time(&self) -> RefTime;

    fn max_forecast_hour(&self) -> u16;

    fn step(&self) -> u16;

    // fn forecasts() -> BTreeMap<ForecastTime, Vec<Stamp>>;
}

pub trait ProviderSpec<S: Storage>: Provider {

    fn storage_provider(&self) -> S;

    async fn start(&self) {
        info!("{} - Start provider", self.id());

        loop {
            info!("{} - Main provider loop", self.id());
            self.clean().await;
            self.download().await;
            sleep(300000).await;
        }
    }

    async fn download(&self);

    async fn download_at(&self, ref_time: RefTime);

    async fn on_file_downloaded(&self, content: &[u8], stamp_id: &StampId) -> Result<()> {    

        self.storage_provider().save(stamp_id.file_name()).await?;

        Ok(())
    }

    async fn on_stamp_downloaded(&self, delete: bool, load: bool, stamp: StampId) -> Result<()> {

        if delete {
          if self.contains(&stamp) && stamp.forecast_hour() > 6 { // keep previous forecast to merge
            self.remove_forecast(&stamp).await;
          }
        }
    
        self.set_last(stamp.ref_time, stamp.forecast_hour(), self.max_forecast_hour()).await;
    
        debug!("Load `{}` {}", stamp, stamp.file_name());
        // stamp.wind  = Some(Arc::new(self.load_stamp(&stamp).await?.try_into()?));
    
        // self.add_forecast(stamp).await;
    
        debug!("{} - Status : {}", self.id(), self.status().read().unwrap());
    
        Ok(())
    }
    
    async fn clean(&self) {

        let status = self.status();
        let mut status = status.write().unwrap();

        while let Some((_, stamps)) = status.forecasts.extract_if(|forecast, _| forecast.from_now() < Duration::hours(-3)).next() {
            for stamp in stamps {
                info!("{} - Delete {}", self.id(), stamp.id);
                match self.storage_provider().remove(stamp.file_name()).await {
                    Ok(()) => {},
                    Err(e) => error!("{} - Error removing file {} : {}", self.id(), stamp.file_name(), e),
                }
            }
        }
    }

    async fn get_last(&self) -> Option<StampId> {
        self.status().read().unwrap().last.clone()
    }

    async fn set_last(&self, ref_time: DateTime<Utc>, forecast_time: u16, max_forecast_time: u16) {
        let status = self.status();
        let mut it = status.write().unwrap();
    
        if it.last.is_none() || it.last.as_ref().unwrap().ref_time <= ref_time {
            it.last = Some(StampId::from((&ref_time, forecast_time)));
            it.progress = (100 * forecast_time / max_forecast_time) as u8;
        }
    }

    async fn get_progress(&self) -> u8 {
        self.status().read().unwrap().progress
    }

    async fn add_forecast(&self, forecast: Stamp) {
        self.status().write().unwrap().forecasts.entry(forecast.id.forecast_time).or_insert(Vec::new()).push(forecast);
    }
    
    async fn remove_forecast(&self, stamp_id: &StampId) {
        self.status().write().unwrap().forecasts.remove(&stamp_id.forecast_time);
    }

    fn contains(&self, stamp_id: &StampId) -> bool {
        self.status().read().unwrap().forecasts.contains_key(&stamp_id.forecast_time)
    }  
}

pub struct ProviderStatus {
    pub(crate) provider: String,
    pub(crate) current_ref_time: RefTime,
    pub(crate) last: Option<StampId>,
    pub(crate) progress: u8,
    pub(crate) forecasts: BTreeMap<ForecastTime, Vec<Stamp>>,
}

impl std::fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      match &self.last {
        Some(last) => {
          write!(f, "{} - `{}Z+{:03}` : {}%", &self.provider, last.ref_time.format("%H"), last.forecast_hour(), &self.progress)
        }
        None => {
          write!(f, "{} : {}%", &self.provider, &self.progress)
        }
      }
    }
  }

