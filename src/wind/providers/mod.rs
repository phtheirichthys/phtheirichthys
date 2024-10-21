use std::{collections::HashMap, sync::{Arc, RwLock}};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use log::{debug, error, info};

use crate::position::Coords;

use self::config::ProviderConfig;

use super::{Provider, ProviderStatus, Wind};

pub(crate) mod config;
mod storage;
pub(crate) mod vr;

pub(crate) struct Providers {
    providers: Arc<RwLock<HashMap<String, Arc<dyn Provider + Sync + Send>>>>,
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
            ProviderConfig::Noaa(_) => todo!(),
            // ProviderConfig::Noaa(config) => {
            //     let noaa = Noaa::from_config(config);
            //     // let winds = noaa.load(true, false).await?;
            //     //noaa.init().await;
            //     wasm_bindgen_futures::spawn_local(async move {
            //       noaa.start().await;
            //     });        
            // },
            ProviderConfig::Vr => {
                let providers = self.providers.clone();
                //wasm_bindgen_futures::spawn_local(async move {
                    match vr::VrWindProvider::new().await {
                        Ok(vr) => {
                            vr.start();

                            let mut providers: std::sync::RwLockWriteGuard<HashMap<String, Arc<dyn Provider + Sync + Send>>> = providers.write().unwrap();
                            providers.insert("vr".into(), Arc::new(vr));
                        },
                        Err(e) => {
                            error!("Failed starting vr wind provider : {}", e);
                        }
                    }
                //});
            }
        }

        Ok(())
    }

    pub(crate) fn get(&self, provider: String) -> ProviderResult {
        let providers: std::sync::RwLockReadGuard<HashMap<String, Arc<dyn Provider + Sync + Send>>> = self.providers.read().unwrap();

        match providers.get(&provider) {
            Some(provider) => {
                let p = provider.clone();
                Ok(p)
            },
            None => {
                bail!("Provider not found")
            },
        }
    }

    pub(crate) fn get_wind(&self, provider: String, m: DateTime<Utc>, point: Coords) -> Result<Wind> {
        let providers: std::sync::RwLockReadGuard<HashMap<String, Arc<dyn Provider + Sync + Send>>> = self.providers.read().unwrap();

        match providers.get(&provider) {
            Some(provider) => {
                Ok(provider.find(&m).interpolate(&point))
            },
            None => {
                bail!("Provider not found")
            },
        }
    }

    pub(crate) fn get_status(&self, provider: String) -> Result<ProviderStatus> {
        debug!("Get provider {provider} status");

        let providers: std::sync::RwLockReadGuard<HashMap<String, Arc<dyn Provider + Sync + Send>>> = self.providers.read().unwrap();

        match providers.get(&provider) {
            Some(provider) => {
                Ok(provider.status())
            },
            None => {
                bail!("Provider not found")
            },
        }
    }

    pub(crate) fn draw(&self, provider: String, m: DateTime<Utc>, x: i64, y: i64, z: u32, width: usize, height: usize, f: Box<dyn FnOnce(&Vec<u8>) -> Result<()> + 'static>) -> Result<()> {
        debug!("Draw wind {provider} ({x},{y},{z}) ({width},{height})");

        let providers: std::sync::RwLockReadGuard<HashMap<String, Arc<dyn Provider + Sync + Send>>> = self.providers.read().unwrap();

        match providers.get(&provider) {
            Some(provider) => {
                debug!("Found provider");

                //let f = Box::new(|data| f(data));
                provider.find(&m).draw(x, y, z, width, height, f)
            },
            None => {
                bail!("Provider not found")
            },
        }
    }
}

type ProviderResult<'a> = Result<Arc<dyn Provider + Sync + Send>>;

pub(crate) trait ProviderResultSpec {
    fn then<F, R>(self, then: F) -> Result<R>
where
    F: FnOnce(Arc<dyn Provider + Send + Sync>) -> Result<R>;
}

impl<'a> ProviderResultSpec for ProviderResult<'a> {
    fn then<F, R>(self, then: F) -> Result<R>
    where
        F: FnOnce(Arc<dyn Provider + Send + Sync>) -> Result<R> {

            match self {
                Ok(p) => {
                    then(p)
                },
                Err(e) => Err(e)
            }
    }
}