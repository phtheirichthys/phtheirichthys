#![feature(btree_extract_if)]

pub(crate) mod algorithm;
pub(crate) mod land;
pub mod phtheirichthys;
pub(crate) mod polar;
pub mod position;
pub(crate) mod race;
mod router;
mod utils;
pub mod wind;

use chrono::{DateTime, TimeZone, Utc};
use log::{error, Level};
use once_cell::sync::Lazy;
use phtheirichthys::Phtheirichthys;
use position::Point;
use wasm_bindgen::prelude::*;
use web_sys::js_sys;
use wind::{providers::{config::ProviderConfig, Providers}, ProviderStatus, Wind};

use crate::{position::Heading, router::RouteRequest};
//use wind::providers::{config::{NoaaProviderConfig, ProviderConfig}, storage::StorageConfig, Providers, ProvidersSpec};

// static PHTHEIRICHTHYS: std::sync::RwLock<Option<Phtheirichthys>> = std::sync::RwLock::new(None);
static PHTHEIRICHTHYS: Lazy<std::sync::RwLock<Phtheirichthys>> = Lazy::new(|| {
    std::sync::RwLock::new(Phtheirichthys::new())
});

#[wasm_bindgen(start)]
fn run() {
    let _ = console_log::init_with_level(Level::Debug);
}

#[wasm_bindgen]
pub async fn add_wind_provider() {
    PHTHEIRICHTHYS.read().unwrap().add_wind_provider().await;
}

#[wasm_bindgen]
pub fn get_wind_provider_status(provider: String) -> Result<JsValue, JsValue> {
    match PHTHEIRICHTHYS.read().unwrap().get_wind_provider_status(provider) {
        Ok(status) => Ok(serde_wasm_bindgen::to_value(&status)?),
        Err(e) => Err(js_sys::Error::new(&e.to_string()))?,
    }
}

#[wasm_bindgen]
pub fn get_wind(provider: String, m: js_sys::Date, point: JsValue) -> Result<JsValue, JsValue> {
    let m = Utc.timestamp_millis_opt(m.get_time() as i64).unwrap();
    let point = serde_wasm_bindgen::from_value(point)?;

    match PHTHEIRICHTHYS.read().unwrap().get_wind(provider, m, point) {
        Ok(status) => Ok(serde_wasm_bindgen::to_value(&status)?),
        Err(e) => Err(js_sys::Error::new(&e.to_string()))?,
    }
}

#[wasm_bindgen]
pub fn eval_snake(route_request: JsValue, params: JsValue, heading: JsValue) -> Result<JsValue, JsValue> {
    let route_request = serde_wasm_bindgen::from_value(route_request)?;
    let params = serde_wasm_bindgen::from_value(params)?;
    let heading = serde_wasm_bindgen::from_value(heading)?;

    match PHTHEIRICHTHYS.read().unwrap().eval_snake(route_request, params, heading) {
        Ok(positions) => Ok(serde_wasm_bindgen::to_value(&positions)?),
        Err(e) => Err(js_sys::Error::new(&e.to_string()))?,
    }
}

#[wasm_bindgen]
pub fn add_polar(name: String, polar: JsValue) -> Result<(), JsValue> {
    let polar = serde_wasm_bindgen::from_value(polar)?;

    PHTHEIRICHTHYS.read().unwrap().add_polar(name, polar);

    Ok(())
}
