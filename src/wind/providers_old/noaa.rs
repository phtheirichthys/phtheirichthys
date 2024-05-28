use anyhow::{bail, Result};
use reqwest::StatusCode;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{js_sys::{self, Promise, Uint8Array}, Request, RequestInit, RequestMode, Response};
use std::{collections::BTreeMap, sync::{Arc, RwLock}};
use std::ops::Neg;
use wasm_bindgen::prelude::{wasm_bindgen, UnwrapThrowExt as _};

use chrono::{DateTime, Duration, Utc};

use crate::{debug, error, info, wind::stamp::{Durations, ForecastTime, ForecastTimeSpec, RefTime, RefTimeSpec, Stamp, StampId}};

use super::{config::NoaaProviderConfig, storage::{web_sys::LocalStorage, Storage}, Provider, ProviderSpec, ProviderStatus};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = fetch)]
    fn fetch_with_request(input: &web_sys::Request) -> Promise;
}

pub struct Noaa {
    id: String,
    pub(crate) status: Arc<RwLock<ProviderStatus>>,
}

impl Noaa {
    pub(crate) fn from_config(config: &NoaaProviderConfig) -> Self {
        info!("Create Noaa provider from config {:?}", config);

        Noaa {
            id: "nooa".into(),
            status: Arc::new(RwLock::new(ProviderStatus {
                provider: "noaa".into(),
                current_ref_time: Utc::now(),
                last: None,
                progress: 0,
                forecasts: BTreeMap::new(),
            })),
        }
    }

    pub(crate) fn current_ref_time() -> RefTime {
        let mut ref_time = RefTime::now();
        if Utc::now() < Self::next_update_time() {
            ref_time = ref_time - 6.hours();
        }

        ref_time
    }

    fn next_update_time() -> DateTime<Utc> {
        let ref_time = RefTime::now();

        ref_time + Duration::hours(3) + Duration::minutes(30)
    }

    async fn download_first(&self, ref_time: RefTime) -> Result<bool> {
        self.download_next(true, ref_time).await
    }

    async fn download_next(&self, first: bool, ref_time: RefTime) -> Result<bool> {

        debug!("download_next {}", ref_time);

        let mut something_new = false;

        let mut ref_time = ref_time;
        let mut h = 6;
        let mut first = first;

        debug!("while {} <= {}", h, self.max_forecast_hour());

        while h <= self.max_forecast_hour() {
            debug!("while {} <= {}", h, self.max_forecast_hour());

            let forecast_time = ForecastTime::from_ref_time(&ref_time, h);

            if forecast_time.from_now() <= self.step().hours().neg() {
                h += self.step();
                continue;
            }

            let stamp_id: StampId = StampId::from((&ref_time, forecast_time));

            if !self.storage_provider().exists(stamp_id.file_name()).await? {

                match self.download_grib(&stamp_id).await {
                    Ok(true) => {
                        something_new = true;
                        self.on_stamp_downloaded(true, false, stamp_id).await?;
                    },
                    Ok(false) => {
                        if first {
                            ref_time = (ref_time - 6.hours()).into();
                            h = 6;
                            first = false;
                            continue
                        }
                        break;
                    }
                    Err(e) => {
                        error!("Error downloading grib `{}` : {:?}", stamp_id, e);
                        break;
                    }
                }
            }

            h += self.step();
            first = false;
        }

        Ok(something_new)
    }

    fn js_fetch(req: &web_sys::Request) -> Promise {
        use wasm_bindgen::{JsCast, JsValue};
        let global = js_sys::global();
    
        if let Ok(true) = js_sys::Reflect::has(&global, &JsValue::from_str("ServiceWorkerGlobalScope"))
        {
            global
                .unchecked_into::<web_sys::ServiceWorkerGlobalScope>()
                .fetch_with_request(req)
        } else {
            // browser
            fetch_with_request(req)
        }
    }

    // pub(crate) async fn download_grib(&self, stamp_id: &StampId) -> Result<bool> {

    //     let url = web_sys::Url::new("https://nomads.ncep.noaa.gov/cgi-bin/filter_gfs_1p00.pl").unwrap();
    //     url.search_params().append("dir", format!("/gfs.{}/{}/atmos", stamp_id.ref_time.format("%Y%m%d"), stamp_id.ref_time.format("%H")).as_str());
    //     url.search_params().append("file", format!("gfs.t{}z.pgrb2.1p00.f{:03}", stamp_id.ref_time.format("%H"), stamp_id.forecast_hour()).as_str());
    //     url.search_params().append("lev_10_m_above_ground", "on");
    //     url.search_params().append("var_UGRD", "on");
    //     url.search_params().append("var_VGRD", "on");
    //     url.search_params().append("leftlon", "0");
    //     url.search_params().append("rightlon", "360");
    //     url.search_params().append("toplat", "90");
    //     url.search_params().append("bottomlat", "-90");


    //     let mut opts = RequestInit::new();
    //     opts.method("GET");
    //     //opts.mode(RequestMode::NoCors);

    //     //TODO : add timeout 30 secs
        
    //     let request = match Request::new_with_str_and_init(&url.href(), &opts) {
    //         Ok(request) => request,
    //         Err(e) => bail!("Building request failed : {:?}", e)
    //     };

    //     debug!("`{}` Try to download {}", stamp_id, request.url());

    //     match JsFuture::from(Self::js_fetch(&request)).await {
    //         Ok(response) => {
    //             assert!(response.is_instance_of::<Response>());
    //             let resp: Response = response.dyn_into().unwrap();

    //             match resp.status() {
    //                 200 => {
    //                     let content = match resp.array_buffer() {
    //                         Ok(buf) => {
    //                             let buf = match JsFuture::from(buf).await {
    //                                 Ok(buf) => buf,
    //                                 Err(e) => bail!("Failed resolving array buffer promise")
    //                             };

    //                             let buffer = Uint8Array::new(&buf);
    //                             let mut bytes = vec![0; buffer.length() as usize];
    //                             buffer.copy_to(&mut bytes);
    //                             bytes
    //                         }
    //                         Err(e) => bail!("Get body as array buffer failed : {:?}", e)
    //                     };

    //                     match self.on_file_downloaded(content.as_ref(), stamp_id).await {
    //                         Ok(()) => {
    //                             info!("`{}` Downloaded", stamp_id);

    //                             Ok(true)
    //                         }
    //                         Err(e) => {
    //                             Err(e)
    //                         }
    //                     }
    //                 }
    //                 404 => {
    //                     debug!("Download failed `{}` : {}", stamp_id, 404);
    //                     Ok(false)
    //                 }
    //                 any => {
    //                     bail!("Download failed `{}` : {} {:?}", stamp_id, any, resp.status_text())
    //                 }
    //             }
    //         },
    //         Err(e) => bail!("Fetching request failed : {:?}", e)
    //     }
    // }

    pub(crate) async fn download_grib(&self, stamp_id: &StampId) -> Result<bool> {

        let url = format!("https://nomads.ncep.noaa.gov/cgi-bin/filter_gfs_1p00.pl");

        let client = reqwest::Client::builder()
            
            //.timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let req = client
            .get(url)
            // .fetch_mode_no_cors()
            .query(&[
                ("dir", format!("/gfs.{}/{}/atmos", stamp_id.ref_time.format("%Y%m%d"), stamp_id.ref_time.format("%H")).as_str()),
                ("file", format!("gfs.t{}z.pgrb2.1p00.f{:03}", stamp_id.ref_time.format("%H"), stamp_id.forecast_hour()).as_str()),
                ("lev_10_m_above_ground", "on"),
                ("var_UGRD", "on"),
                ("var_VGRD", "on"),
                ("leftlon", "0"),
                ("rightlon", "360"),
                ("toplat", "90"),
                ("bottomlat", "-90"),
            ])
            .build()?;

        debug!("`{}` Try to download {}", stamp_id, req.url());

        match client.execute(req).await {
            Ok(response) => {
                debug!("Downloaded");
                match response.status() {
                    StatusCode::OK => {
                        debug!("Download OK");

                        let content = response.bytes().await?;

                        match self.on_file_downloaded(content.as_ref(), stamp_id).await {
                            Ok(()) => {
                                info!("`{}` Downloaded", stamp_id);

                                Ok(true)
                            }
                            Err(e) => {
                                Err(e)
                            }
                        }
                    },
                    StatusCode::NOT_FOUND => {
                        debug!("Download Failed Not Found");
                        debug!("Download failed `{}` : {}", stamp_id, StatusCode::NOT_FOUND);
                        Ok(false)
                    },
                    any => {
                        debug!("Download Failed {}", any);
                        bail!("Download failed `{}` : {}", stamp_id, any);
                    }
                }
            },
            Err(e) => {
                bail!("Error downloading grib file {} : {:?}", stamp_id, e);
            }
        }
    }
}

impl Provider for Noaa {    
    fn id(&self) -> String {
        self.id.clone()
    }

    fn status(&self) -> Arc<RwLock<ProviderStatus>> {
        self.status.clone()
    }

    fn current_ref_time(&self) -> RefTime {
        Self::current_ref_time()
    }
    
    fn max_forecast_hour(&self) -> u16 {
        384
    }
    
    fn step(&self) -> u16 {
        3
    }
    
}

impl ProviderSpec<LocalStorage> for Noaa {
    fn storage_provider(&self) -> LocalStorage {
        LocalStorage { prefix: "__".into() }
    }

    async fn download(&self) {
        let ref_time = self.current_ref_time();
        self.download_at(ref_time).await;
    }

    async fn download_at(&self, ref_time: RefTime) {
        debug!("Is there something to download ?");

        match self.download_first(ref_time).await {
            Ok(something_new) => {
                debug!("Nothing more to download for now");
                if something_new {
                    let last: crate::wind::stamp::StampId = self.get_last().await.expect("the last");
                    info!("`{}Z+{:03}` : {}%", last.ref_time.format("%H"), last.forecast_hour(), self.get_progress().await);
                }
            },
            Err(e) => {
                error!("An error occurred while trying to download : {:?}", e);
            }
        }
    }
}