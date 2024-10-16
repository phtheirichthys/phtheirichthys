use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::f64::consts::PI;
use std::fmt;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use chrono_humanize::{Accuracy, Tense, HumanTime};
use log::{debug, error, info};

#[cfg(feature = "rayon")]
use rayon::prelude::*;

use crate::race;
use crate::{polar::Polar, polar::PolarCache, race::Race, router};
use crate::algorithm::Algorithm;
use crate::algorithm::spherical::Spherical;
use crate::phtheirichthys::BoatOptions;
use crate::land::LandsProvider;
use crate::position::{Heading, Penalties, Coords, Sail, BoatSettings, BoatStatus};
use crate::router::{IsochroneSection, Router, RouteInfos, RouteRequest, RouteResult, WaypointStatus, Wind, Isochrone, IsochronePoint};
use crate::utils::{Distance, Speed};
use crate::wind::{InstantWind, Provider};

pub(crate) struct Echeneis<A: 'static + Algorithm + Send + Sync> {
    bot_name: String,
    winds: Arc<dyn Provider + Send + Sync>,
    lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>,
    polar: Arc<Polar>,
    algorithm: Arc<A>,
    config: EcheneisConfig,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EcheneisConfig {
    pub(crate) accuracy: f64,
    pub(crate) display_all_isochrones: bool,
    pub(crate) timeout: u64,
}

#[async_trait]
impl<A: Algorithm + Send + Sync> Router for Echeneis<A> {

    async fn route(&self, race: &Race, boat_options: BoatOptions, request: RouteRequest, routing_timeout: Option<Duration>) -> Result<RouteResult> {

        let start_routing = Utc::now();

        debug!("Route asked : {:?}", request);

        let boat_options = Arc::new(boat_options);

        let max_duration: Duration = Duration::hours(20*24); //Duration::minutes(25); //

        let steps = request.steps.clone();

        let start = request.start_time;

        let mut from = request.from.clone();
        let mut froms = Nav {
            absolute_duration: Duration::zero(),
            min: None,
            alternatives: BTreeMap::from([(0, request.clone().into())]),
            reached_by_way: false,
            crossed: false,
        };
        let mut now = start.clone();
        let mut duration = Duration::zero();

        let mut sections = Vec::new();

        let mut best = None;
        let mut best_dist_to: Distance;

        let mut success = true;

        let mut future_navs: VecDeque<Nav> = VecDeque::new();

        let mut deb = Vec::new();

        let mut buoys = get_buoys(race, from.clone()).peekable();
        let mut max = BTreeMap::new();

        while let Some(mut destination) = buoys.next() {

            let mut reached = false;
            let min = destination.distance(&from);
            let max_radius = if min.clone() / 1000.0 < Distance::from_nm(1000.0) {
                min.clone() * 1.5
            } else if min.clone() / 1000.0 < Distance::from_nm(100.0) {
                min.clone() * 2.0
            } else {
                min.clone() * 1.5
            };

            best_dist_to = min.clone();

            let factor = self.get_factor(&from, &destination);

            self.debug(format!("Route to {} at {}", destination.name(), factor));

            let mut section = IsochroneSection {
                door: destination.name().clone(),
                isochrones: Vec::new(),
            };

            while !reached && success && duration < max_duration && !routing_timeout.is_some_and(|timeout| Utc::now() > start_routing.add(timeout.clone())) {

                let (_, step) = steps.iter().filter(|(d, _)| d > &duration).next().unwrap_or(steps.last().unwrap());

                // prepare

                while let Some(future_nav) = future_navs.front() {
                    if future_nav.absolute_duration < duration + step.clone() {
                        future_navs.pop_front();
                    } else {
                        break;
                    }
                }

                self.debug(format!("Navigate {} ({}) {} ->", HumanTime::from(step.clone()).to_text_en(Accuracy::Rough, Tense::Present), HumanTime::from(duration).to_text_en(Accuracy::Precise, Tense::Present), froms.size()));

                /*for futur_nav in futur_navs.iter() {
                    debug!("\t{} : {}", HumanTime::from(futur_nav.absolute_duration).to_text_en(Accuracy::Precise, Tense::Present), futur_nav.size());
                }*/

                // let mut navs = match timeout(
                    // std::time::Duration::from_secs(self.config.timeout),
                let mut navs = self.navigate2(&boat_options, &from, &now, froms, &mut destination, step.clone(), factor, &mut max, &max_radius, future_navs.to_owned()).await;
                // ).await {
                //     Err(_) => {
                //         bail!("timeout while navigate");
                //     }
                //     Ok(res) => res,
                // };

                debug!("->");
                for nav in navs.iter() {
                    debug!("\t{} : {}", HumanTime::from(nav.absolute_duration).to_text_en(Accuracy::Precise, Tense::Present), nav.size());
                }

                if let Some(nav) = navs.pop_front() {

                    reached = nav.reached_by_way;
                    duration = nav.absolute_duration;

                    // Generate isochrone for ui
                    {
                        let hours = duration.num_minutes();

                        if self.config.display_all_isochrones || hours % 60 < step.num_minutes() {
                            let color = if hours % 1440 < step.num_minutes() {
                                "%24".to_string()
                            } else if hours % 360 < step.num_minutes() {
                                "%6".to_string()
                            } else if hours % 60 < step.num_minutes() {
                                "%1".to_string()
                            } else {
                                "%0".to_string()
                            };

                            section.isochrones.push(Isochrone {
                                color,
                                paths: nav.to_isochrone(self.config.display_all_isochrones),
                            });
                        }
                    }

                    // Get overall best result
                    {
                        for (az, alternative) in nav.alternatives.iter() {
                            for variant in alternative.variants.iter() {
                                if let Some(pos) = variant {

                                    if !best.as_ref().is_some_and(|_| pos.dist_to >= best_dist_to) {
                                        best = Some(Arc::new(pos.clone()));
                                        best_dist_to = pos.dist_to.clone();
                                    }

                                    if pos.reached.is_some() {
                                        deb.push(IsochronePoint {
                                            lat: pos.point.lat.clone(),
                                            lon: pos.point.lon.clone(),
                                            az: az.clone(),
                                            previous: 0,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // Is boat arrived
                    if nav.crossed && buoys.peek().is_none() {
                        // TODO : arrived
                        // Search for better route (cross line / cross circle)
                        reached = true
                    }

                    let reached = nav.reached_by_way;

                    now = request.start_time + duration;
                    froms = nav;

                    if reached {
                        let reachers = destination.reachers();

                        self.debug(format!("reached buoy : {:?}", reachers.iter().map(|r| format!("{}:{}", r.size(), HumanTime::from(r.absolute_duration.clone()).to_text_en(Accuracy::Rough, Tense::Present))).collect::<Vec<String>>()));

                        if reachers.len() == 0 {
                            success = false;
                            break;
                        }

                        // generate futur_navs from reachers
                        navs = {
                            let previous_factor = factor;
                            let factor = match buoys.peek() {
                                Some(buoy) => {
                                    self.get_factor(&destination.departure(), &buoy)
                                },
                                None => factor
                            };

                            let mut navs = HashMap::new();
                            for previous in reachers[1..].to_vec() {

                                let nav = navs.entry(previous.absolute_duration).or_insert_with(|| Nav::from(previous.absolute_duration));

                                for (az, a) in previous.alternatives.iter() {
                                    let az = (az.clone() as f64 / previous_factor * factor).round() as i32;

                                    let mut alternative = Alternative::empty();
                                    for s in 0..8 {
                                        alternative.variants[s] = a.variants[s].clone().map(|p| {
                                            let mut p = p.clone();
                                            p.dist_to = destination.distance(&p.point);
                                            p
                                        });
                                    }
                                    nav.alternatives.insert(az, alternative);
                                }
                            }

                            let mut navs = navs.iter().map(|(_, nav)| nav.to_owned()).collect::<Vec<Nav>>();
                            navs.sort_by(|a, b| a.absolute_duration.cmp(&b.absolute_duration));
                            navs.into()
                        };


                        let nav = reachers.first().unwrap();

                        max.clear();
                        for (az, alternative) in nav.alternatives.iter() {
                            for s in 0..8 {
                                alternative.variants[s].as_ref().map(|p| {
                                    max.entry(*az).or_insert_with(|| [Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero()])[s] = p.from_dist.clone() * 1.001;
                                });
                            }
                        }

                        duration = nav.absolute_duration;
                        now = request.start_time + duration;
                        froms = nav.clone();
                    }

                    if froms.size() == 0 && navs.iter().map(|nav| nav.size()).sum::<usize>() == 0 {
                        success = false
                    }

                    future_navs = navs;

                } else {
                    bail!("no nav found");
                }
            }

            from = destination.departure();
            sections.push(section);

            if !reached {
                break;
            }
        }

        let mut way = Vec::new();

        if let Some(last) = best {
            way.push(router::RouteWaypoint {
                from: last.point.clone(),
                duration: last.duration.absolute.clone(),
                way_duration: Duration::zero(),
                boat_settings: Default::default(),
                status: WaypointStatus {
                    boat_speed: Default::default(),
                    wind: Default::default(),
                    foil: 0,
                    boost: 0,
                    best_ratio: 0.0,
                    ice: false,
                    change: false,
                    penalties: Vec::new(),
                    remaining_penalties: Vec::new(),
                    stamina: 0.0,
                    remaining_stamina: 0.0,
                }
            });

            let mut next = last;
            while let Some(last) = next.previous.as_ref() {
                way.push(router::RouteWaypoint {
                    from: last.point.clone(),
                    duration: last.duration.absolute,
                    way_duration: next.duration.relative.clone(),
                    boat_settings: next.settings.clone(),
                    status: WaypointStatus {
                        boat_speed: next.status.boat_speed.clone(),
                        wind: next.status.wind.clone(),
                        foil: next.status.foil,
                        boost: next.status.boost,
                        best_ratio: next.status.best_ratio,
                        ice: false,
                        change: false,
                        penalties: next.status.penalties.clone().into(),
                        remaining_penalties: next.remaining_penalties.clone().into(),
                        stamina: next.status.stamina,
                        remaining_stamina: next.remaining_stamina,
                    }
                });
                next = last.clone();
            }
        } else {
            bail!("Routing failed");
        }

        way.sort_by(|a, b| a.duration.cmp(&b.duration));

        Ok(RouteResult {
            infos: RouteInfos {
                start,
                duration: 0.0,
                success,
                sails_duration: HashMap::new(),
                foil_duration: 0.0
            },
            way,
            sections,
            debug: deb,
        })
    }
}

impl<A: 'static + Algorithm + Send + Sync> Echeneis<A> {

    pub(crate) fn new(bot_name: String, polar: Arc<Polar>, winds: Arc<dyn Provider + Send + Sync>, lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>, algorithm: Arc<A>, config: EcheneisConfig) -> Self {
        debug!("[{}] Create new Echeneis Router", bot_name);
        Self {
            bot_name,
            winds,
            lands_provider,
            polar,
            algorithm,
            config,
        }
    }

    pub(crate) fn jump2(algorithm: &Arc<A>,
                        lands_provider: Option<&Arc<Box<dyn LandsProvider + Send + Sync>>>,
                        polar: &mut PolarCache,
                        boat_options: &Arc<BoatOptions>,
                        start: &Arc<Coords>,
                        from: &Arc<Position>,
                        to: &Option<Arc<Buoy>>,
                        heading: &Heading,
                        duration: Duration,
                        wind: &Wind,
                        factor: f64) -> Vec<(i32, Position)> {

        let twa = heading.twa(wind.direction);
        if twa.abs() < 30.0 || twa.abs() > 160.0 {
            return Vec::new()
        }

        polar.get_boat_speeds(&heading, wind, &from.settings.sail, from.is_in_ice_limits, false).into_iter().map(|polar_result| {
            let penalties = polar.add_penalties(boat_options, from.remaining_penalties.clone(), from.remaining_stamina,
                                                from.settings.heading.twa(from.status.wind.direction), twa,
                                                &from.settings.sail, &polar_result.sail,
                                                &wind.speed
            );

            let mut jump_duration = duration;
            if penalties.duration() > duration {
                jump_duration = jump_duration * ((penalties.duration().num_minutes() as f64 / jump_duration.num_minutes() as f64).ceil() as i32);
            }

            let (distance, remaining_penalties, boat_speed, ratio) = Polar::distance(polar_result.speed, jump_duration, &penalties);

            let stamina = polar.tired(from.remaining_stamina, from.settings.heading.twa(from.status.wind.direction), twa,
                                      &from.settings.sail, &polar_result.sail,
                                      &wind.speed);

            let remaining_stamina = polar.recovers(stamina, &jump_duration, &wind.speed);

            let point = algorithm.destination(&from.point, heading.heading(wind.direction), &distance);

            if lands_provider.is_some() && lands_provider.unwrap().is_land(point.lat, point.lon) {
                return None;
            }

            let (from_dist, az) = algorithm.distance_and_heading_to(&*start, &point);

            let dist_to = to.as_ref().map_or(Distance::zero(), |to| to.distance(&point));

            let az = (az * factor).round() as i32;
            Some((az, Position {
                az,
                point,
                from_dist,
                dist_to,
                duration: from.duration.clone() + jump_duration,
                distance,
                reached: None,
                settings: BoatSettings {
                    heading: heading.clone(),
                    sail: polar_result.sail,
                },
                status: BoatStatus {
                    aground: false,
                    boat_speed,
                    wind: wind.clone(),
                    foil: polar_result.foil,
                    boost: polar_result.boost,
                    best_ratio: polar_result.best,
                    ratio: (ratio * 100.0) as u8,
                    vmgs: None,
                    penalties,
                    stamina,
                },
                previous: Some(from.clone()),
                is_in_ice_limits: false, //TODO manage ice
                remaining_penalties,
                remaining_stamina,
            }))
        }).filter(|alt| alt.is_some()).map(|alt| alt.unwrap()).collect()
    }

    fn buoy_reached(algorithm: &Arc<A>, polar: &mut PolarCache, boat_options: &Arc<BoatOptions>, start: &Arc<Coords>, from: &Arc<Position>, to: &Arc<Buoy>, duration: Duration, wind: &Wind, factor: f64) -> Option<(i32, Position)> {

        if from.dist_to > from.distance.clone() * 10.0 {
            return None;
        }

        let (distance, heading) = algorithm.distance_and_heading_to(&from.point, &to.destination());

        let heading = Heading::HEADING(heading);

        let mut results = Vec::new();

        for polar_result in polar.get_boat_speeds(&heading, wind, &from.settings.sail, from.is_in_ice_limits, false).into_iter() {
            let penalties = polar.add_penalties(boat_options, from.remaining_penalties.clone(), from.remaining_stamina,
                                                from.settings.heading.twa(from.status.wind.direction), heading.twa(wind.direction),
                                                &from.settings.sail, &polar_result.sail,
                                                &wind.speed
            );

            let (duration_to_buoy, remaining_penalties, boat_speed, ratio) = Polar::duration(polar_result.speed, distance.clone(), penalties.clone());

            let stamina = polar.tired(from.remaining_stamina,
                                      from.settings.heading.twa(from.status.wind.direction), heading.twa(wind.direction),
                                      &from.settings.sail, &polar_result.sail,
                                      &wind.speed
            );

            let remaining_stamina = polar.recovers(stamina, &duration_to_buoy, &wind.speed);

            if duration_to_buoy.num_seconds() as f64 <= duration.num_seconds() as f64 * 1.5 {

                let (from_dist, az) = algorithm.distance_and_heading_to(&*start, &to.destination());

                let az = (az * factor).round() as i32;
                results.push((az.clone(), Position {
                    az,
                    point: to.destination(),
                    from_dist,
                    dist_to: Distance::zero(),
                    duration: from.duration.clone() + duration_to_buoy,
                    distance: distance.clone(),
                    reached: if to.is_waypoint() { Some(to.name().clone()) } else { None },
                    settings: BoatSettings {
                        heading: heading.clone(),
                        sail: polar_result.sail,
                    },
                    status: BoatStatus {
                        aground: false,
                        boat_speed,
                        wind: wind.clone(),
                        foil: polar_result.foil,
                        boost: polar_result.boost,
                        best_ratio: polar_result.best,
                        ratio: (ratio * 100.0) as u8,
                        vmgs: None,
                        penalties,
                        stamina,
                    },
                    previous: Some(from.clone()),
                    is_in_ice_limits: false,
                    remaining_penalties,
                    remaining_stamina,
                }));
            }

        }

        results.sort_by(|(_, a), (_, b)| a.duration.cmp(&b.duration));
        results.into_iter().next()
    }

    fn way2(algorithm: Arc<A>,
            lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>,
            polar: &mut PolarCache,
            boat_options: Arc<BoatOptions>,
            start: Arc<Coords>,
            from: Arc<Position>, to: &Option<Arc<Buoy>>,
            duration: Duration,
            wind: &Wind,
            factor: f64) -> Vec<Nav> {

        if to.is_some() {
            let to = to.as_ref().unwrap();
            let reached = Self::buoy_reached(&algorithm, polar, &boat_options, &start, &from, to, duration, wind, factor);
            if let Some((_, pos)) = reached {
                return vec!(Nav{
                    absolute_duration: pos.duration.absolute,
                    min: None,
                    alternatives: BTreeMap::from([(-1, Alternative::from(pos))]),
                    reached_by_way: true,
                    crossed: false,
                });
            }
        }

        let mut navs:  BTreeMap<Duration, Nav> = BTreeMap::new();
        let mut default_nav = Nav::from((from.duration.clone() + duration).absolute);

        // If near land : heading. Else twa
        // if lands_provider.near_land(from.point.lat, from.point.lon) {
        //     for heading in 0..360 {
        //         let heading = Heading::HEADING(heading as f64);
        //         let positions = Self::jump2(&algorithm, Some(&lands_provider), &polar, &boat_options, &start, &from, to, &heading, duration, wind, factor);
        //
        //         for (az, pos) in positions {
        //             let nav = navs.entry(pos.duration.absolute).or_insert_with(|| Nav::from(pos.duration.absolute));
        //             nav.min = nav.min.to_owned().or(Some(pos.dist_to.clone())).and_then(|min| if min < pos.dist_to { Some(min) } else { Some(pos.dist_to.clone()) });
        //             let alternative = nav.alternatives.entry(az).or_insert_with(|| Alternative::empty());
        //             alternative.merge_fast(pos);
        //         }
        //     }
        // } else {
            for twa in -180..180 {
                let heading = Heading::TWA(twa as f64);
                let positions = Self::jump2(&algorithm, Some(&lands_provider), polar, &boat_options, &start, &from, to, &heading, duration, wind, factor);

                for (az, pos) in positions {
                    let nav = if pos.duration.relative == duration { &mut default_nav } else { navs.entry(pos.duration.absolute).or_insert_with(|| Nav::from(pos.duration.absolute)) };
                    {
                        // nav.min = nav.min.to_owned().or(Some(pos.dist_to.clone())).and_then(|min| if min < pos.dist_to { Some(min) } else { Some(pos.dist_to.clone()) });
                        let new_min = match &nav.min {
                            None => { true }
                            Some(min) if { min > &pos.dist_to } => { true }
                            _ => { false }
                        };

                        if new_min {
                            nav.min = Some(pos.dist_to.clone())
                        }
                    }
                    let alternative = nav.alternatives.entry(az).or_insert_with(|| Alternative::empty());
                    alternative.merge_fast(pos);
                }
            }
        // }

        let mut navs = if navs.len() > 0 {
            let mut navs = navs.iter().map(|(_, nav)| nav.to_owned()).collect::<Vec<Nav>>();
            navs.push(default_nav);
            navs.sort_by(|a, b| a.absolute_duration.cmp(&b.absolute_duration));
            navs
        } else {
            vec![default_nav]
        };
        navs
    }

    async fn navigate2(&self, boat_options: &Arc<BoatOptions>, start: &Coords, now: &DateTime<Utc>, from: Nav, to: &mut Buoy, duration: Duration, factor: f64, max: &mut BTreeMap<i32, [Distance;8]>, max_radius: &Distance, navs: VecDeque<Nav>) -> VecDeque<Nav> {

        let navs = Arc::new(Mutex::new(navs.into_iter().map(|nav| (nav.absolute_duration, nav)).collect::<HashMap<Duration, Nav>>()));

        let winds = Arc::new(self.winds.find(now));
        let algorithm = self.algorithm.clone();
        let lands_provider = self.lands_provider.clone();
        let polar = self.polar.clone();
        let boat_options = boat_options.clone();
        let start = Arc::new(start.clone());

        Self::navigate_from_all(from, to, duration, factor, &navs, winds, algorithm, lands_provider, polar, boat_options, start).await;

        let navs = navs.lock().unwrap();
        debug!("{:?}", navs.keys());
        let mut navs = navs.iter().map(|(_, nav)| nav.to_owned()).collect::<Vec<Nav>>();
        navs.sort_by(|a, b| a.absolute_duration.cmp(&b.absolute_duration));

        navs.first_mut().map(|nav| {

            if nav.reached_by_way && (to.is_door() || to.is_zone()) {
                // check if the buoy was cross
                // if not : add the destination as point from were to go
                if to.reachers().len() == 0 {
                    match nav.alternatives.get(&-1) {
                        Some(alternative) => {
                            match alternative.best() {
                                Some(best) => {
                                    to.reach(&best, factor);
                                }
                                None => {}
                            }
                        },
                        None => {}
                    }
                }
            }

            if !nav.reached_by_way {

                let mut size = 0;
                for (_, alternative) in nav.alternatives.iter() {
                    size += alternative.variants.iter().filter(|v| v.is_some()).count();
                }

                let double_min = nav.min.clone().map(|min| min * 2.0);

                for (az, alternative) in nav.alternatives.iter_mut() {

                    let best_from_dist = alternative.best().map_or(Distance::zero(), |b| b.from_dist.clone());
                    let best_sail = alternative.best().map_or(0, |b| b.settings.sail.index);

                    // if this was already reached before
                    if max.get(az).is_some_and(|d| best_from_dist < d.get(best_sail).unwrap_or(&Distance::zero())) {
                        for s in 0..8 {
                            if alternative.variants[s].is_some() {
                                alternative.variants[s] = None;
                                size -= 1;
                            }
                        }
                        continue;
                    }

                    for s in 0..8 {
                        match alternative.variants.get(s) {
                            Some(Some(pos)) => {

                                // check if pos is to avoid
                                if to.is_to_avoid(&pos.point) {
                                    alternative.variants[s] = None;
                                    size -= 1;
                                    continue;
                                }

                                // check if too far from route
                                if pos.from_dist.m() + pos.dist_to.m() > max_radius.m() {
                                    alternative.variants[s] = None;
                                    size -= 1;
                                    continue;
                                }

                                // check if not going too far from min reached point (if remains enough points)
                                match &double_min {
                                    Some(double_min) => {
                                        if size > 25 && pos.dist_to > double_min {
                                            alternative.variants[s] = None;
                                            size -= 1;
                                            continue;
                                        }
                                    },
                                    _ => {}
                                }

                                // check if too far from best alternative
                                if pos.from_dist.clone() + pos.status.boat_speed.clone() * Duration::minutes(300) < best_from_dist {
                                    alternative.variants[s] = None;
                                    size -= 1;
                                    continue;
                                }

                                max.entry(*az).or_insert_with(|| [Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero(), Distance::zero()])[s] = pos.from_dist.clone() * 1.001;
                            }
                            _ => {}
                        }
                    }

                }

                nav.alternatives.retain(|_, alternative| {

                    for s in 0..8 {
                        if alternative.variants[s].is_some() {
                            return true
                        }
                    }

                    false
                });

                // check if the door was crossed
                for (_, alternative) in nav.alternatives.iter_mut() {
                    for s in 0..8 {
                        match alternative.variants.get(s) {
                            Some(Some(pos)) => {
                                if to.crossed(pos) {
                                    nav.crossed = true;
                                    alternative.variants[s] = Some(pos.reached(to))
                                }
                            },
                            _ => {}
                        }
                    }
                }

                for (_, alternative) in nav.alternatives.iter() {
                    for s in 0..8 {
                        match alternative.variants.get(s) {
                            Some(Some(pos)) => {
                                let mut parent_reached = false;
                                let mut p = pos;
                                for _ in 0..10 {
                                    if let Some(prev) = p.previous.as_ref() {
                                        p = prev;
                                        if p.reached.as_ref().is_some_and(|n| n == to.name()) {
                                            parent_reached = true;
                                            break
                                        }
                                    } else {
                                        break
                                    }
                                }

                                if pos.reached.as_ref().is_some_and(|n| n == to.name()) || parent_reached {
                                    to.reach(pos, factor);
                                }
                            },
                            _ => {}
                        }
                    }
                }

            }
        });

        navs.into()
    }

    #[cfg(feature = "rayon")]
    async fn navigate_from_all(from: Nav, to: &mut Buoy, duration: Duration, factor: f64, navs: &Arc<Mutex<HashMap<Duration, Nav>>>, winds: Arc<Box<dyn InstantWind + Send + Sync>>, algorithm: Arc<A>, lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>, polar: Arc<Polar>, boat_options: Arc<BoatOptions>, start: Arc<Coords>) {
        let (send, recv) = tokio::sync::oneshot::channel();
        {
            let navs = navs.clone();
            let winds = winds.clone();
            let to = Arc::new(to.clone());

            rayon::spawn(move || {
                from.alternatives.par_iter().for_each(|(_, alternative)| {
                    Self::navigate_from_alternative(duration, factor, algorithm.clone(), lands_provider.clone(), polar.clone(), boat_options.clone(), start.clone(), navs.clone(), winds.clone(), to.clone(), alternative);
                });

                let _ = send.send(());
            });
        }

        recv.await.expect("Panic in rayon::spawn");
    }

    #[cfg(not(feature = "rayon"))]
    async fn navigate_from_all(from: Nav, to: &mut Buoy, duration: Duration, factor: f64, navs: &Arc<Mutex<HashMap<Duration, Nav>>>, winds: Arc<Box<dyn InstantWind + Send + Sync>>, algorithm: Arc<A>, lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>, polar: Arc<Polar>, boat_options: Arc<BoatOptions>, start: Arc<Coords>) {
        let navs = navs.clone();
        let winds = winds.clone();
        let to = Arc::new(to.clone());

        from.alternatives.iter().for_each(|(_, alternative)| {
            Self::navigate_from_alternative(duration, factor, algorithm.clone(), lands_provider.clone(), polar.clone(), boat_options.clone(), start.clone(), navs.clone(), winds.clone(), to.clone(), alternative);
        });
    }

    fn navigate_from_alternative(duration: Duration, factor: f64, algorithm: Arc<A>, lands_provider: Arc<Box<dyn LandsProvider + Send + Sync>>, polar: Arc<Polar>, boat_options: Arc<BoatOptions>, start: Arc<Coords>, navs: Arc<Mutex<HashMap<Duration, Nav>>>, winds: Arc<Box<dyn InstantWind + Send + Sync>>, to: Arc<Buoy>, alternative: &Alternative) {
        let mut polar = PolarCache::new(polar);

        alternative.variants.iter().for_each(|variant| {
            variant.as_ref().map(|variant| {
                let navs = navs.clone();
                let winds = winds.clone();
                // let variant = variant.clone();
                let algorithm = algorithm.clone();
                let lands_provider = lands_provider.clone();
                let boat_options = boat_options.clone();
                let start = start.clone();
                let to = to.clone();

                let wind = winds.interpolate(&variant.point);

                let way_navs = Self::way2(algorithm, lands_provider, &mut polar, boat_options, start, Arc::new(variant.clone()), &Some(to), duration, &wind, factor);

                for way_nav in way_navs {
                    if way_nav.reached_by_way {
                        let mut navs = navs.lock().unwrap();

                        // remove all navs later than current
                        navs.retain(|d, _| d <= &way_nav.absolute_duration);

                        let nav = navs.entry(way_nav.absolute_duration).or_insert_with(|| Nav::from(way_nav.absolute_duration));

                        if !nav.reached_by_way {
                            nav.alternatives.clear();
                            nav.reached_by_way = true;
                            nav.min = None;
                        }

                        let prev = nav.alternatives.entry(-1).or_insert_with(|| Alternative::empty());
                        prev.merge_all_by_duration(way_nav.alternatives.get(&-1).unwrap().clone());

                        break;
                    } else {
                        let mut navs = navs.lock().unwrap();

                        let nav = navs.entry(way_nav.absolute_duration).or_insert_with(|| Nav::from(way_nav.absolute_duration));

                        if !nav.reached_by_way {
                            nav.min = match (nav.min.to_owned(), way_nav.min.to_owned()) {
                                (None, way_nav_min) => way_nav_min,
                                (nav_min, None) => nav_min,
                                (Some(nav_min), Some(way_nav_min)) => Some(nav_min.min(way_nav_min)),
                            };

                            for (az, alternative) in way_nav.alternatives {
                                let prev = nav.alternatives.entry(az).or_insert_with(|| Alternative::empty());
                                prev.merge_all(alternative);
                            }
                        }
                    }
                }
            });
        });
    }

    fn get_factor(&self, from: &Coords, to: &Buoy) -> f64 {
        let dist = to.distance(from);
        let polar_result = self.polar.get_boat_speed(&Heading::TWA(90.0), &Wind { direction: 0.0 ,speed: Speed::from_kts(10.0) }, Some(&Sail::from_index(0)), &Sail::from_index(0), false);
        let dist_between_points = polar_result.speed.km_h() * 3.0 * 1000.0;
        
        self.config.accuracy + ((PI/180.0)/(dist_between_points /dist.m()).clamp(-1.0, 1.0).asin()).round()
    }

    fn debug(&self, msg: String) {
      debug!("[{}] {}", self.bot_name, msg);
    }
    
    fn _info(&self, msg: String) {
      info!("[{}] {}", self.bot_name, msg);
    }
    
    fn _error(&self, msg: String) {
      error!("[{}] {}", self.bot_name, msg);
    }
}

#[derive(Clone, Debug)]
struct Nav {
    absolute_duration: Duration,
    min: Option<Distance>,
    alternatives: BTreeMap<i32, Alternative>,
    reached_by_way: bool,
    crossed: bool,
}

impl Nav {

    fn from(absolute_duration: Duration) -> Self {
        Nav {
            absolute_duration,
            min: None,
            alternatives: BTreeMap::new(),
            reached_by_way: false,
            crossed: false,
        }
    }

    fn size(&self) -> usize {

        let mut size = 0;

        for (_, alternative) in self.alternatives.iter() {
            size += alternative.variants.iter().filter(|v| v.is_some()).count();
        }

        size
    }

    fn to_isochrone(&self, display_all: bool) -> Vec<Vec<IsochronePoint>> {
        let mut azs = self.alternatives.keys().collect::<Vec<&i32>>();
        azs.sort_by(|a, b| a.cmp(b));

        let mut paths = Vec::new();
        let mut path = Vec::new();

        let mut previous_az = -99;
        for az in azs {
            if az - previous_az > 6 {
                paths.push(path);
                path = Vec::new();
            }

            if let Some(best) = self.alternatives[az].best() {

                let mut previous = -1;
                let mut p = &best;
                while let Some(parent) = &p.previous {
                    if parent.visible(display_all) {
                        previous = parent.az;
                        break;
                    }
                    p = parent;
                }

                path.push(IsochronePoint {
                    lat: best.point.lat.clone(),
                    lon: best.point.lon.clone(),
                    az: az.clone(),
                    previous,
                });
                previous_az = *az;
            }
        }

        if path.len() > 0 {
            paths.push(path);
        }

        paths

    }
}

#[derive(Clone, Copy, Debug, Hash)]
pub(crate) struct NavDuration {
    absolute: Duration,
    relative: Duration,
}

impl NavDuration {
    pub(crate) fn zero() -> Self {
        NavDuration {
            absolute: Duration::zero(),
            relative: Duration::zero(),
        }
    }
}

impl PartialEq<Self> for NavDuration {
    fn eq(&self, other: &Self) -> bool {
        self.absolute == other.absolute
    }
}

impl PartialOrd<Self> for NavDuration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for NavDuration {}

impl Ord for NavDuration {
    fn cmp(&self, other: &Self) -> Ordering {
        self.absolute.cmp(&other.absolute)
    }
}

impl Add<Duration> for NavDuration {
    type Output = Self;

    fn add(self, duration: Duration) -> Self::Output {
        Self {
            absolute: self.absolute + duration,
            relative: duration,
        }
    }
}

#[derive(Clone, Debug)]
struct Alternative {
    variants: [Option<Position>;8],
}

impl Alternative {
    fn empty() -> Self {
        Alternative {
            variants: [None, None, None, None, None, None, None, None],
        }
    }

    fn empty_boxed() -> Box<Self> {
        Box::new(Alternative {
            variants: [None, None, None, None, None, None, None, None],
        })
    }

    fn from(pos: Position) -> Self {
        let mut res = Self::empty();
        res.merge_fast(pos);
        res
    }

    fn merge_all(&mut self, alternative: Alternative) {

        for s in 0..8 {
            if let Some(pos) = &alternative.variants[s] {
                if self.variants[s].is_none() || pos.better_than(self.variants[s].as_ref().unwrap()) {
                    /*if pos.az == 679 && pos.settings.sail.index == 6 {
                        let previous_from_dist = pos.previous.as_ref().map_or(&pos.from_dist, |p| &p.from_dist);
                        debug!("NEW BEST {:?}({} - {} - {})", pos, pos.from_dist, previous_from_dist, pos.remaining_penalties.total().num_seconds());
                    }*/

                    self.variants[s] = Some(pos.clone());
                } else {
                    /*if pos.az == 679 && pos.settings.sail.index == 6 {
                        let previous_from_dist = pos.previous.as_ref().map_or(&pos.from_dist, |p| &p.from_dist);
                        debug!("--- {} - {} - {} (+{})", pos.from_dist, previous_from_dist, pos.remaining_penalties.total().num_seconds(), self.variants[s].as_ref().unwrap().nav_duration.num_minutes());
                    }*/
                }
            }
        }
    }

    fn merge_all_by_duration(&mut self, alternative: Alternative) {

        for s in 0..8 {
            if let Some(pos) = &alternative.variants[s] {
                if !self.variants[s].as_ref().is_some_and(|a| a.duration <= pos.duration) {
                    self.variants[s] = Some(pos.clone());
                }
            }
        }
    }

    fn merge_fast(&mut self, pos: Position) {
        let sail_index = pos.settings.sail.index.clone();
        let sail_index = 0;

        let renew = match &self.variants[sail_index] {
            None => { true }
            Some(variant) if { variant.from_dist < &pos.from_dist } => { true }
            _ => { false }
        };

        if renew {
            self.variants[sail_index] = Some(pos);
        }
    }

    fn merge(&mut self, pos: Position) {
        let sail_index = pos.settings.sail.index.clone();
        let sail_index = 0;

        if self.variants[sail_index].is_none() || pos.better_than(self.variants[sail_index].as_ref().unwrap()) {
            self.variants[sail_index] = Some(pos);
        }
    }

    fn best(&self) -> Option<Position> {

        let mut best = None;

        for s in 0..8 {
            self.variants[s].as_ref().map(|v| {
                let best = best.get_or_insert_with(|| self.variants[s].clone().unwrap());
                if v.from_dist > best.from_dist {
                    *best = v.clone();
                }
            });
        }

        best
    }

    fn _get(&self, sail: usize) -> Option<Position> {

        let mut best = None;

        self.variants[sail].as_ref().map(|v| {
            let best = best.get_or_insert_with(|| self.variants[sail].clone().unwrap());
            if v.from_dist > best.from_dist {
                *best = v.clone();
            }
        });

        best
    }
}

impl From<RouteRequest> for Alternative {
    fn from(route_request: RouteRequest) -> Self {

        let mut variants = [None, None, None, None, None, None, None, None];
        let sail_index = route_request.boat_settings.sail.index;
        variants[sail_index] = Some(route_request.into());

        Alternative {
            variants
        }
    }
}


#[derive(Clone)]
pub(crate) struct Position {
    pub(crate) az: i32,
    pub(crate) point: Coords,
    pub(crate) from_dist: Distance,
    pub(crate) dist_to: Distance,
    pub(crate) duration: NavDuration,
    pub(crate) distance: Distance,
    pub(crate) reached: Option<String>,
    pub(crate) settings: BoatSettings,
    pub(crate) status: BoatStatus,
    pub(crate) previous: Option<Arc<Position>>,
    pub(crate) is_in_ice_limits: bool,
    pub(crate) remaining_penalties: Penalties,
    pub(crate) remaining_stamina: f64,
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Position")
            .field("az", &self.az)
            .field("settings", &self.settings)
            .field("boat_speed", &self.status.boat_speed)
            .field("from_dist", &self.from_dist)
            .field("remaining_penalties", &self.remaining_penalties)
            .finish()
    }
}

impl PartialEq for Position {
    fn eq(&self, other: &Self) -> bool {
        self.az == other.az
    }
}

unsafe impl Send for Position {}
unsafe impl Sync for Position {}

impl Position {
    fn reached(&self, buoy: &Buoy) -> Self {
        let mut reached = self.clone();
        reached.reached = Some(buoy.name().clone());
        reached
    }

    fn visible(&self, display_all: bool) -> bool {
        display_all || self.duration.absolute.num_minutes() % 60 < self.duration.relative.num_minutes()
    }

    fn better_than(&self, other: &Position) -> bool {

        // ancetre commun : le meilleur est celui avec la plus courte penalité de voile
        /*if self.common_ancestor(other).is_some() {
            if self.remaining_penalties.sail_change.as_ref().map_or(0, |p| p.duration.num_minutes()) < other.remaining_penalties.sail_change.as_ref().map_or(0, |p| p.duration.num_minutes()) {
                return true
            } else if self.remaining_penalties.sail_change.as_ref().map_or(0, |p| p.duration.num_minutes()) > other.remaining_penalties.sail_change.as_ref().map_or(0, |p| p.duration.num_minutes()) {
                return false
            }
        }

        self.from_dist > other.from_dist*/


        /*if self.status.best_ratio >= 0.99 {
            let nb_navs = ((self.remaining_penalties.duration() - other.remaining_penalties.duration()).num_seconds() as f64 / self.duration.relative.num_seconds() as f64) as i64;

            if nb_navs > 0 {
                return self.from_dist.clone() + self.distance.clone() * (nb_navs as f64) > other.from_dist.clone() + other.distance.clone() * (nb_navs as f64);
            } else if nb_navs < 0 {
                return self.from_dist.clone() + self.distance.clone() * (nb_navs as f64).abs() >= other.from_dist.clone() + other.distance.clone() * (nb_navs as f64).abs();
            }
        }*/

        /*
        let self_previous_from_dist = self.previous.as_ref().map_or(&self.from_dist, |p| &p.from_dist);
        let other_previous_from_dist = other.previous.as_ref().map_or(&other.from_dist, |p| &p.from_dist);

        /*if self.az == 706 && self.settings.sail.index == 6 && other.remaining_penalties.total().num_seconds() < 360 {
            error!("{} <= {} - {} && {} + {} > {} = {}", other.remaining_penalties.total().num_seconds(), self.remaining_penalties.total().num_seconds(), other.nav_duration.num_seconds(), self.from_dist, other.from_dist.clone(), (self.from_dist.clone() - self_previous_from_dist), other.remaining_penalties.total() <= self.remaining_penalties.total() - self.nav_duration / 2 && other.from_dist.clone() + (self.from_dist.clone() - self_previous_from_dist) >= self.from_dist.clone());
        }*/

        // si penalitées restantes sont plus petites d'au moins une nav
        let nb_navs = (self.remaining_penalties.total() - other.remaining_penalties.total()).num_seconds() as f64 / self.nav_duration.num_seconds() as f64;
        if nb_navs > 0.0 && other.from_dist.clone() + (self.from_dist.clone() - self_previous_from_dist) * nb_navs >= self.from_dist.clone() {

            return false
        }

        let nb_navs = (other.remaining_penalties.total() - self.remaining_penalties.total()).num_seconds() as f64 / other.nav_duration.num_seconds() as f64;
        if self.from_dist.clone() + (other.from_dist.clone() - other_previous_from_dist) * nb_navs >= other.from_dist.clone() {

            return true
        }
        */

        self.from_dist > other.from_dist
    }

    fn _common_ancestor(&self, other: &Arc<Position>) -> Option<Arc<Position>> {
        for _ in 0..10 {
            if self.previous == other.previous {
                return self.previous.clone()
            }
        }

        None
    }
}

impl From<RouteRequest> for Position {
    fn from(route_request: RouteRequest) -> Self {

        Position {
            az: 0,
            point: route_request.from.clone(),
            from_dist: Distance::zero(),
            dist_to: Distance::zero(),
            duration: NavDuration::zero(),
            distance: Distance::zero(),
            reached: None,
            settings: route_request.boat_settings.clone(),
            status: route_request.status.clone(),
            previous: None,
            is_in_ice_limits: false,
            remaining_penalties: route_request.status.penalties.clone(),
            remaining_stamina: route_request.status.stamina,
        }
    }
}


fn get_buoys(race: &Race, boat: Coords) -> impl Iterator<Item = Buoy> {
    let w = race.buoys.clone();
    w.into_iter().filter(|w| !w.is_validated())
        .map(move |w| Buoy::from(w, boat.clone()))
}

#[derive(Clone)]
pub(crate) struct Buoy {
    inner: race::Buoy,
    reachers: Vec<Nav>,
}

impl Buoy {

    fn from(buoy: race::Buoy, _boat: Coords) -> Self {
        Self {
            inner: buoy,
            reachers: Vec::new(),
        }
    }

    fn departure(&self) -> Coords {
        match &self.inner {
            race::Buoy::Door(door) => {
                door.departure.clone()
            }
            race::Buoy::Waypoint(waypoint) => { waypoint.destination.clone() }
            race::Buoy::Zone(zone) => { zone.destination.clone() }
        }
    }

    fn destination(&self) -> Coords {
        match &self.inner {
            race::Buoy::Door(door) => { door.destination.clone() }
            race::Buoy::Waypoint(waypoint) => { waypoint.destination.clone() }
            race::Buoy::Zone(zone) => { zone.destination.clone() }
        }
    }

    fn name(&self) -> &String {
        match &self.inner {
            race::Buoy::Door(door) => { &door.name }
            race::Buoy::Waypoint(waypoint) => { &waypoint.name }
            race::Buoy::Zone(zone) => { &zone.name }
        }
    }

    fn is_to_avoid(&self, point: &Coords) -> bool {
        let to_avoids = match &self.inner {
            race::Buoy::Door(door) => { &door.to_avoid }
            race::Buoy::Waypoint(waypoint) => { &waypoint.to_avoid }
            race::Buoy::Zone(zone) => { &zone.to_avoid }
        };

        for t in to_avoids {
            let as_x = point.lat - t.0.lat;
            let as_y = point.lon - t.0.lon;

            let s_ab = (t.1.lat-t.0.lat)*as_y-(t.1.lon-t.0.lon)*as_x > 0.0;

            if ((t.2.lat-t.0.lat)*as_y-(t.2.lon-t.0.lon)*as_x > 0.0) == s_ab {
                continue
            }

            if ((t.2.lat-t.1.lat)*(point.lon-t.1.lon)-(t.2.lon-t.1.lon)*(point.lat-t.1.lat) > 0.0) != s_ab {
                continue
            }

            return true
        }

        false
    }

    fn distance(&self, to: &Coords) -> Distance {
        match &self.inner {
            race::Buoy::Door(door) => {
                Spherical{}.distance_to(&door.destination, to)
            }
            race::Buoy::Waypoint(waypoint) => {
                Spherical{}.distance_to(&waypoint.destination, to)
            }
            race::Buoy::Zone(zone) => {
                Spherical{}.distance_to(&zone.destination, to) - &zone.radius
            }
        }
    }

    fn _distance_and_heading_to(&self, to: &Coords) -> (Distance, f64) {
        match &self.inner {
            race::Buoy::Door(door) => {
                Spherical{}.distance_and_heading_to(&door.destination, to)
            }
            race::Buoy::Waypoint(waypoint) => {
                Spherical{}.distance_and_heading_to(&waypoint.destination, to)
            }
            race::Buoy::Zone(zone) => {
                let (distance, heading) = Spherical{}.distance_and_heading_to(&zone.destination, to);
                (distance - &zone.radius, heading)
            }
        }
    }

    fn crossed(&self, pos: &Position) -> bool {
        let algorithm = Spherical{};
        match &self.inner {
            race::Buoy::Door(door) => {
                if let Some(src) = &pos.previous {
                    let t = algorithm.heading_to(&door.port, &door.starboard);
                    let a = algorithm.heading_to(&src.point, &door.port);

                    let alpha = 180.0 + a - t;

                    let b = algorithm.heading_to(&src.point, &door.starboard);
                    let beta = b - t;

                    let heading = pos.settings.heading.heading(pos.status.wind.direction);

                    if b < t
                        && (a < b && heading > a && heading < b
                            || a > b && (heading > a || heading < b)) {

                        let a2 = algorithm.heading_to(&pos.point, &door.port);
                        let mut alpha2 = 180.0 + a2 - t;
                        if a2 > 180.0 {
                            alpha2 = alpha2 - 360.0
                        }
                        let b2 = algorithm.heading_to(&pos.point, &door.starboard);
                        let beta2 = b2 - t;

                        return alpha*alpha2 < 0.0 && beta*beta2 < 0.0;
                    }
                }

                false
            },
            race::Buoy::Zone(zone) => {
                if let Some(src) = &pos.previous {
                    !zone.is_in(&src.point) && zone.is_in(&pos.point)
                } else {
                    false
                }
            },
            _ => false
        }
    }

    fn reach(&mut self, pos: &Position, factor: f64) {

        let (dist, az) = Spherical{}.distance_and_heading_to(&self.departure(), &pos.point);

        let reachers = match self.inner {
            race::Buoy::Waypoint(_) => {
                return;
            }
            _ => {
                &mut self.reachers
            }
        };

        if let Some(last) = reachers.last_mut() {
            if last.absolute_duration != pos.duration.absolute {
                reachers.push(Nav {
                    absolute_duration: pos.duration.absolute,
                    min: None,
                    alternatives: Default::default(),
                    reached_by_way: false,
                    crossed: false,
                });
            }
        } else {
            reachers.push(Nav {
                absolute_duration: pos.duration.absolute,
                min: None,
                alternatives: Default::default(),
                reached_by_way: false,
                crossed: false,
            });
        }

        let last = reachers.last_mut().unwrap();

        // TODO : should be factor of next buoy
        let az = (az * factor).round() as i32;
        let alternative = last.alternatives.entry(az).or_insert_with(|| Alternative::empty());

        alternative.merge(Position {
            az,
            point: pos.point.clone(),
            from_dist: dist,
            dist_to: Distance::zero(),
            duration: pos.duration.clone(),
            distance: Distance::zero(),
            reached: None,
            settings: pos.settings.clone(),
            status: pos.status.clone(),
            previous: pos.previous.clone(),
            is_in_ice_limits: false,
            remaining_penalties: pos.remaining_penalties.clone(),
            remaining_stamina: pos.remaining_stamina,
        });
    }

    fn reachers(&self) -> Vec<Nav> {
        self.reachers.clone()
    }

    fn is_waypoint(&self) -> bool {
        match self.inner {
            race::Buoy::Door(_) => false,
            race::Buoy::Waypoint(_) => true,
            race::Buoy::Zone(_) => false,
        }
    }

    fn is_zone(&self) -> bool {
        match self.inner {
            race::Buoy::Door(_) => false,
            race::Buoy::Waypoint(_) => false,
            race::Buoy::Zone(_) => true
        }
    }

    fn is_door(&self) -> bool {
        match self.inner {
            race::Buoy::Door(_) => true,
            race::Buoy::Waypoint(_) => false,
            race::Buoy::Zone(_) => false
        }
    }
}

