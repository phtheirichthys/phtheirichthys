//#![cfg(target_arch = "wasm32")]

extern crate wasm_bindgen_test;

use log::{debug, Level};
use wasm_bindgen_test::*;

use crate::wind::{providers::vr::VrWindProvider, Provider};

wasm_bindgen_test_configure!(run_in_browser);

// #[wasm_bindgen_test]
// async fn noaa() {    
//     let noaa = Noaa::from_config(&NoaaProviderConfig { enabled: true, gribs: StorageConfig::WebSys { prefix: "__".into() } });

//     let ref_time = Noaa::current_ref_time();
//     let forecast_time = ForecastTime::from_ref_time(&ref_time, 6);
//     let stamp_id: StampId = StampId::from((&ref_time, forecast_time));
//     let res = noaa.download_grib(&stamp_id).await;

//     debug!("Res : {:?}", res);
// }

#[wasm_bindgen_test]
async fn vr() {
    console_log::init_with_level(Level::Debug);

    debug!("Testing VrWindProvider ...");

    let vr = match VrWindProvider::new().await {
        Ok(vr) => vr,
        Err(e) => panic!("Error building VrWindProvider : {}", e)
    };

    debug!("Start VrWindProvider");

    vr.start();
}
