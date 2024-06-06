use std::sync::Arc;
use anyhow::Result;

use chrono::{DateTime, Duration, Utc};
use log::error;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::race::{Race, Races, RacesSpec};
use crate::{polar::{Polar, Polars, PolarsSpec}, position::{Heading, Penalties, Coords, Settings, Status}, router::{echeneis::{Echeneis, NavDuration, Position}, RouteRequest}, utils::Distance, wind::{providers::{config::ProviderConfig, ProviderResultSpec as _, Providers}, ProviderStatus, Wind}};

pub struct Phtheirichthys {
    providers: Providers,
    polars: Polars,
    races: Races,
}

impl Phtheirichthys {

    pub(crate) fn new() -> Self {
        Phtheirichthys {
            providers: Providers::new(),
            polars: <Polars as PolarsSpec>::new(),
            races: <Races as RacesSpec>::new(),
        }
    }

    pub(crate) async fn add_wind_provider(&self) {
        //self.providers.init_provider(&ProviderConfig::Noaa(NoaaProviderConfig { enabled: true, gribs: StorageConfig::WebSys { prefix: "__".into() } }));
        match self.providers.init_provider(&ProviderConfig::Vr).await {
            Ok(()) => {},
            Err(e) => error!("Failed adding provider : {}", e)
        }
    }

    pub(crate) fn get_wind_provider_status(&self, provider: String) -> anyhow::Result<ProviderStatus> {
        self.providers.get_status(provider)
    }

    pub(crate) fn get_wind(&self, provider: String, m: DateTime<Utc>, point: Coords) -> anyhow::Result<Wind> {
        self.providers.get_wind(provider, m, point)
    }

    pub(crate) fn add_polar(&self, name: String, polar: Polar) {
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

    pub(crate) fn eval_snake(&self, route_request: RouteRequest, params: SnakeParams, heading: Heading) -> Result<Vec<(i64, Coords)>> {
        let wind_provider = self.providers.get(params.wind_provider)?;
        let start = Arc::new(route_request.from.clone());
        let polar = self.polars.get(&params.polar)?;
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
                &polar,
                &boat_options.clone(),
                &start,
                &Arc::new(src),
                &None,
                &t, Duration::hours(1), &wind, 1.0
            );

            src = jump.iter().map(|(_, pos)| pos).max_by_key(|pos| &pos.distance).unwrap().to_owned();

            result.push((duration.num_hours(), src.point.clone()));

            duration += delta;
            now += delta;
            winds = wind_provider.find(&now);
            wind = winds.interpolate(&src.point);
        }

        Ok(result)
    }
}

#[derive(Serialize, Deserialize)]
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