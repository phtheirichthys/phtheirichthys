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

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use log::Level;
use once_cell::sync::Lazy;
use phtheirichthys::Phtheirichthys;
use position::Coords;
use race::Race;
use serde::{Deserialize, Serialize};
use tsify::{declare, Tsify};
use wasm_bindgen::prelude::*;
use web_sys::js_sys::{self, Array};
use wind::{providers::{config::ProviderConfig, Providers}, ProviderStatus, Wind};

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

#[derive(Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct VecRaces {
    #[tsify(type = "[Race, ...Race[]]")]
    pub vec: Vec<Race>,
}

#[wasm_bindgen]
pub fn list_races() -> VecRaces {
    VecRaces {
        vec: PHTHEIRICHTHYS.read().unwrap().list_races()
    }
}

#[wasm_bindgen]
pub fn get_race(name: String) -> Result<Race, JsValue> {
    match PHTHEIRICHTHYS.read().unwrap().get_race(name) {
        Ok(race) => Ok(race),
        Err(e) => Err(js_sys::Error::new(&e.to_string()))?,
    }
}

#[wasm_bindgen]
pub fn set_race(name: String, race: Race) {
    PHTHEIRICHTHYS.read().unwrap().set_race(name, race)
}