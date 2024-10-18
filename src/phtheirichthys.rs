use std::sync::Arc;
use anyhow::{bail, Result};

use chrono::{DateTime, Duration, Utc};
use cubecl::prelude::*;
// use gloo::timers::callback::Timeout;
use log::error;
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use crate::{algorithm, land, wind};
use crate::land::vr::VrLandProvider;
use crate::race::{Race, Races, RacesSpec};
use crate::router::echeneis::EcheneisConfig;
use crate::router::{RouteResult, Router};
use crate::{polar::{Polar, Polars, PolarsSpec}, position::{Heading, Penalties, Coords}, router::{echeneis::{Echeneis, NavDuration, Position}, RouteRequest}, utils::Distance, wind::{providers::config::ProviderConfig, ProviderStatus, Wind}};
use crate::algorithm::Algorithm;
use crate::polar::PolarCache;

pub struct Phtheirichthys {
    wind_providers: wind::providers::Providers,
    land_providers: land::Providers,
    polars: Polars,
    races: Races,
}

#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SnakeResult {
    positions: Vec<(i64, Coords)>
}

impl Phtheirichthys {

    pub fn new() -> Self {
        Phtheirichthys {
            wind_providers: wind::providers::Providers::new(),
            land_providers: land::Providers::new(),
            polars: <Polars as PolarsSpec>::new(),
            races: <Races as RacesSpec>::new(),
        }
    }

    pub async fn add_wind_provider(&self) {
        //self.providers.init_provider(&ProviderConfig::Noaa(NoaaProviderConfig { enabled: true, gribs: StorageConfig::WebSys { prefix: "__".into() } }));
        match self.wind_providers.init_provider(&ProviderConfig::Vr).await {
            Ok(()) => {},
            Err(e) => error!("Failed adding provider : {}", e)
        }
    }

    pub fn get_wind_provider_status(&self, provider: String) -> anyhow::Result<ProviderStatus> {
        self.wind_providers.get_status(provider)
    }

    pub(crate) fn get_wind(&self, provider: String, m: DateTime<Utc>, point: Coords) -> anyhow::Result<Wind> {
        self.wind_providers.get_wind(provider, m, point)
    }

    pub async fn add_land_provider(&self) {
        //self.providers.init_provider(&ProviderConfig::Noaa(NoaaProviderConfig { enabled: true, gribs: StorageConfig::WebSys { prefix: "__".into() } }));
        match self.land_providers.init_provider(&land::config::ProviderConfig::Vr).await {
            Ok(()) => {},
            Err(e) => error!("Failed adding provider : {}", e)
        }
    }

    pub(crate) fn draw_land(&self, provider: String, x: i64, y: i64, z: u32, width: usize, height: usize, f: Box<dyn FnOnce(&Vec<u8>) -> Result<()> + 'static>) -> Result<()> {
        self.land_providers.draw(provider, x, y, z, width, height, f)
    }

    pub fn add_polar(&self, name: String, polar: Polar) {
        let mut polars = self.polars.write().unwrap();

        polars.insert(name, Arc::new(polar));
    }

    pub(crate) fn list_races(&self) -> Vec<Race> {
        self.races.list()
    }

    pub(crate) fn get_race(&self, name: String) -> Result<Race> {
        self.races.get(&name)
    }

    pub(crate) fn set_race(&self, name: String, race: Race) {
        self.races.set(name, race)
    }

    pub(crate) fn eval_snake(&self, route_request: RouteRequest, params: SnakeParams, heading: Heading) -> Result<SnakeResult> {
        let wind_provider = self.wind_providers.get(params.wind_provider)?;
        let start = Arc::new(route_request.from.clone());
        let mut polar = PolarCache::new(self.polars.get(&params.polar)?);
        let boat_options = Arc::new(params.boat_options);

        let mut now = route_request.start_time;
        let mut duration = Duration::zero();
        let delta = Duration::hours(1);
        let mut winds = wind_provider.find(&now);

        let mut src = Position {
            az: 0,
            point: route_request.from,
            from_dist: Distance::zero(),
            dist_to: Distance::zero(),
            duration: NavDuration::zero(),
            distance: Distance::zero(),
            reached: None,
            settings: route_request.boat_settings,
            status: route_request.status.clone(),
            previous: None,
            is_in_ice_limits: false,
            remaining_penalties: Penalties::new(),
            remaining_stamina: route_request.status.stamina,
        };
        let mut result = vec![(0, src.point.clone())];

        let mut wind = winds.interpolate(&src.point);
        let t = Heading::TWA(heading.twa(wind.direction).round());

        while duration < Duration::hours(params.max_duration) {
            let jump = Echeneis::<_>::jump2(
                &std::sync::Arc::new(crate::algorithm::spherical::Spherical{}),
                None,
                &mut polar,
                &boat_options.clone(),
                &start,
                &Arc::new(src),
                &None,
                &t, Duration::hours(1), &wind, 1.0, true
            );

            src = jump.iter().map(|(_, pos)| pos).max_by_key(|pos| &pos.distance).unwrap().to_owned();

            result.push((duration.num_hours(), src.point.clone()));

            duration += delta;
            now += delta;
            winds = wind_provider.find(&now);
            wind = winds.interpolate(&src.point);
        }

        Ok(SnakeResult { positions: result })
    }

    fn launch<R: Runtime>(device: &R::Device) {

        let start = Utc::now();

        let client = R::client(device);

        let mut vec: Vec<f32> = Vec::with_capacity(1024);
        for _ in 0..vec.capacity() {
            vec.push(rand::random());
        }
        let from_lat = vec.as_slice();
        let mut vec: Vec<f32> = Vec::with_capacity(1024);
        for _ in 0..vec.capacity() {
            vec.push(rand::random());
        }
        let from_lon = vec.as_slice();
        let mut vec: Vec<f32> = Vec::with_capacity(1024);
        for _ in 0..vec.capacity() {
            vec.push(rand::random());
        }
        let to_lat = vec.as_slice();
        let mut vec: Vec<f32> = Vec::with_capacity(1024);
        for _ in 0..vec.capacity() {
            vec.push(rand::random());
        }
        let to_lon = vec.as_slice();

        let output_handle = client.empty(from_lat.len() * core::mem::size_of::<f32>());
        let from_lat_handle = client.create(f32::as_bytes(from_lat));
        let from_lon_handle = client.create(f32::as_bytes(from_lon));
        let to_lat_handle = client.create(f32::as_bytes(to_lat));
        let to_lon_handle = client.create(f32::as_bytes(to_lon));

        unsafe {
            algorithm::cubecl_spherical::distance_to_array::launch_unchecked::<F32, R>(
                &client,
                CubeCount::Static(1, 1, 1),
                CubeDim::new(from_lat.len() as u32, 1, 1),
                ArrayArg::from_raw_parts(&from_lat_handle, from_lat.len(), 1),
                ArrayArg::from_raw_parts(&from_lon_handle, from_lon.len(), 1),
                ArrayArg::from_raw_parts(&to_lat_handle, to_lat.len(), 1),
                ArrayArg::from_raw_parts(&to_lon_handle, to_lon.len(), 1),
                ArrayArg::from_raw_parts(&output_handle, from_lat.len(), 1),
            )
        };

        let bytes = client.read(output_handle.binding());
        let output = f32::from_bytes(&bytes);

        // Should be [-0.1587,  0.0000,  0.8413,  5.0000]
        println!("Executed gelu with runtime {:?} in {:?}ns => {output:?}", R::name(), (Utc::now() - start).num_nanoseconds());

        let start = Utc::now();
        let algo = algorithm::spherical::Spherical {};
        for i in 0..vec.capacity() {
            algo.distance_to(&Coords {lat: from_lat[i] as f64, lon: from_lon[i] as f64 }, &Coords {lat: to_lat[i] as f64, lon: to_lon[i] as f64 });
        }

        println!("Executed loop in {:?}ns", (Utc::now() - start).num_nanoseconds());
    }

    pub fn test_webgpu(&self) -> Result<()> {
        Self::launch::<cubecl::wgpu::WgpuRuntime>(&Default::default());

        Ok(())
    }
    
    pub async fn navigate(&self, wind_provider: String, polar_id: String, race: Race, boat_options: BoatOptions, request: RouteRequest) -> Result<RouteResult> {
        let wind_provider = self.wind_providers.get(wind_provider)?;
        let polar = self.polars.get(&polar_id)?;
        let lands_provider = Arc::new(VrLandProvider::new()?);
        let algorithm = std::sync::Arc::new(crate::algorithm::spherical::Spherical{});

        // let timeout = Timeout::new(0, move || {
        //     wasm_bindgen_futures::spawn_local(async move {
                let router = Echeneis::new("".to_string(), polar, wind_provider, lands_provider, algorithm, EcheneisConfig { accuracy: 1.0, display_all_isochrones: false, timeout: 60 });

                match router.route(&race, boat_options, request, None).await {
                    Ok(result) => {
                        Ok(result)
                    },
                    Err(e) => bail!("Navigation failed : {}", e)
                }
        //     });
        // });

        // timeout.forget();

    }
}

#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct SnakeParams {
    max_duration: i64,
    polar: String,
    wind_provider: String,
    boat_options: BoatOptions,
}

#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BoatOptions {
    pub lt: bool,
    pub gt: bool,
    pub code0: bool,
    pub foil: bool,
    pub hull: bool,
    pub winch: bool,
    pub stamina: bool,
}

impl BoatOptions {
    pub fn new() -> Self {
        Self { lt: false, gt: false, code0: false, foil: false, hull: false, winch: false, stamina: false }
    }
}
