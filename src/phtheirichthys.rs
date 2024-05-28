use std::sync::Arc;
use anyhow::Result;

use chrono::{DateTime, Duration, Utc};
use chrono::serde::ts_seconds;
use log::error;
use serde::{Deserialize, Serialize};

use crate::{polar::{Polar, Polars, PolarsSpec}, position::{Heading, Penalties, Point, Settings, Status}, router::{echeneis::{Echeneis, NavDuration, Position}, RouteRequest}, utils::Distance, wind::{providers::{config::ProviderConfig, ProviderResultSpec as _, Providers}, ProviderStatus, Wind}};

pub struct Phtheirichthys {
    providers: Providers,
    polars: Polars,
}

impl Phtheirichthys {

    pub(crate) fn new() -> Self {
        Phtheirichthys {
            providers: Providers::new(),
            polars: <Polars as PolarsSpec>::new(),
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

    pub(crate) fn get_wind(&self, provider: String, m: DateTime<Utc>, point: Point) -> anyhow::Result<Wind> {
        self.providers.get_wind(provider, m, point)
    }

    pub(crate) fn add_polar(&self, name: String, polar: Polar) {
        let mut polars = self.polars.write().unwrap();

        polars.insert(name, Arc::new(polar));
    }

    pub(crate) fn eval_snake(&self, route_request: RouteRequest, params: SnakeParams, heading: Heading) -> Result<Vec<(i64, Point)>> {
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

        while (duration < Duration::hours(params.max_duration)) {
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

#[derive(Serialize, Deserialize)]
pub(crate) struct BoatOptions {
    pub(crate) lt: bool,
    pub(crate) gt: bool,
    pub(crate) code0: bool,
    pub(crate) foil: bool,
    pub(crate) hull: bool,
    pub(crate) winch: bool,
    pub(crate) stamina: bool,
}