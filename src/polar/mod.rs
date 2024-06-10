use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::{bail, Result};
use chrono::Duration;
use serde::{Serialize, Deserialize};
use tsify::Tsify;
use crate::phtheirichthys::BoatOptions;
use crate::position;
use crate::position::{Heading, Penalties, Penalty};
use crate::utils::{Distance, Speed, SpeedUnit};
use crate::wind::Wind;

pub(crate) type Polars = Arc<RwLock<HashMap<String, Arc<Polar>>>>;

pub(crate) trait PolarsSpec {
    fn new() -> Self;

    fn get(&self, name: &String) -> Result<Arc<Polar>>;
}

impl PolarsSpec for Polars {
    fn new() -> Self {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn get(&self, name: &String) -> Result<Arc<Polar>> {
        let polars = self.read().unwrap();
        match polars.get(name) {
            Some(polar) => Ok(polar.clone()),
            None => bail!("Polar {name} not found"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PolarResult {
    pub(crate) sail: position::Sail,
    pub(crate) speed: Speed,
    pub(crate) foil: u8,
    pub(crate) boost: u8,
    pub(crate) best: f64,
}

impl Polar {
    fn interpolation_index(values: &Vec<f64>, value: f64) -> (usize, usize, f64) {
        let mut i = 0;
        while values[i] < value {
            i += 1;
            if i == values.len() {
                if values[i - 1] < value {
                    return (i - 1, 0, 1.0);
                }
                return (i - 1, i, (values[i] - value) / (values[i] - values[i - 1]));
            }
        }

        if i > 0 {
            return (i - 1, i, (values[i] - value) / (values[i] - values[i-1]));
        }

        (0, 0, 0.0)
    }

    pub(crate) fn get_boat_speeds(&self, heading: &Heading, wind: &Wind, current_sail: &position::Sail, is_in_ice_limits: bool, all: bool) -> Vec<PolarResult> {

        let mut twa = heading.twa(wind.direction);
        if twa < 0.0 {
            twa = -1.0 * twa
        }
        if twa > 180.0 {
            twa = 360.0 - twa
        }

        let tws_indices = Self::interpolation_index(&self.tws, wind.speed.kts());
        let twa_indices = Self::interpolation_index(&self.twa, twa);

        let mut boat_speed_max = Speed::from_kts(0.0);

        // TODO : manage options
        let mut speeds: Vec<(position::Sail, Speed, u8)> = Vec::new();
        for sail in self.sail.iter() {
            let ti0 = &sail.speed[twa_indices.0];
            let ti1 = &sail.speed[twa_indices.1];

            let mut boat_speed = Speed {
                value: (ti0[tws_indices.0] * tws_indices.2 + ti0[tws_indices.1] * (1.0 - tws_indices.2)) * twa_indices.2 + (ti1[tws_indices.0] * tws_indices.2 + ti1[tws_indices.1] * (1.0 - tws_indices.2)) * (1.0 - twa_indices.2),
                unit: SpeedUnit::Knot,
            };

            boat_speed *= self.global_speed_ratio;
            if is_in_ice_limits {
                boat_speed *= self.ice_speed_ratio;
            }
            // TODO : manage hull option
            boat_speed *= self.hull.speed_ratio;
            let foil = self.foil_amount(twa, &wind.speed);
            // TODO : manage foil option
            boat_speed *= foil;

            if boat_speed_max.kts() < boat_speed.kts() {
                boat_speed_max = boat_speed.clone();
            }

            speeds.push((sail.id.into(), boat_speed, ((foil - 1.0) * 100.0 / (self.foil.speed_ratio - 1.0)) as u8));
        }

        speeds.into_iter().map(|(s, boat_speed, foil)| {

            if &s == current_sail {
                let boost = boat_speed_max.kts() / boat_speed.kts();
                if boost <= self.auto_sail_change_tolerance {
                    return PolarResult {
                        sail: s,
                        speed: boat_speed_max.clone(),
                        foil,
                        boost: ((boost - 1.0) * 100.0 / (self.auto_sail_change_tolerance - 1.0)) as u8,
                        best: 1.0
                    }
                }
            }

            PolarResult {
                sail: s,
                speed: boat_speed.clone(),
                foil,
                boost: 0,
                best: boat_speed.kts() / boat_speed_max.kts()
            }
        }).filter(|res| res.best >= if all { 0.0 } else { 0.5 }).collect()
    }

    pub(crate) fn get_boat_speed(&self, heading: &Heading, wind: &Wind, using_sail: Option<&position::Sail>, current_sail: &position::Sail, is_in_ice_limits: bool) -> PolarResult {

        let using_sail = match using_sail {
            Some(position::Sail { auto: true, .. }) => None,
            Some(s) => Some(s),
            None => None
        };

        let mut max_boat_speed: Speed = Default::default();
        let mut best = PolarResult::default();

        for polar_result in self.get_boat_speeds(heading, wind, current_sail, is_in_ice_limits, true).into_iter() {
            if using_sail.as_ref().is_some_and(|using_sail| {
                &&polar_result.sail != using_sail
            }) {
                continue;
            }
            if polar_result.speed > max_boat_speed {
                max_boat_speed = polar_result.speed.clone();
                best = polar_result.clone();
            }
        }

        best
    }

    fn  get_boat_speed_from_wind_index(&self, wind_speed: &Speed, using_sail: Option<&position::Sail>, is_in_ice_limits: bool, tws_indices: (usize, usize, f64), twa: f64) -> (Speed, position::Sail, f64) {
        let (twa_index_0, twa_index_1, twa_factor) = Self::interpolation_index(&self.twa, twa);

        let mut max_boat_speed: Speed = Default::default();
        let mut best_sail= position::Sail::from_index(0);

        for sail in self.sail.iter() {
            if using_sail.is_some_and(|using_sail| sail.id != using_sail.id) {
                continue;
            }

            // TODO : manage options

            let ti0 = &sail.speed[twa_index_0];
            let ti1 = &sail.speed[twa_index_1];

            let boat_speed = Speed {
                value: (ti0[tws_indices.0] * tws_indices.2 + ti0[tws_indices.1] * (1.0 - tws_indices.2)) * twa_factor + (ti1[tws_indices.0] * tws_indices.2 + ti1[tws_indices.1] * (1.0 - tws_indices.2)) * (1.0 - twa_factor),
                unit: SpeedUnit::Knot,
            };

            if boat_speed > max_boat_speed {
                max_boat_speed = boat_speed;
                best_sail = sail.id.into()
            }
        }

        max_boat_speed *= self.global_speed_ratio;
        if is_in_ice_limits {
            max_boat_speed *= self.ice_speed_ratio;
        }
        // TODO : manage hull option
        max_boat_speed *= self.hull.speed_ratio;
        let foil = self.foil_amount(twa, wind_speed);
        // TODO : manage foil option
        max_boat_speed *= foil;

        (max_boat_speed, best_sail, foil)
    }

    pub(crate) fn get_vmg(&self, wind_speed: &Speed, using_sail: Option<&position::Sail>, is_in_ice_limits: bool) -> Vmgs {

        let mut upwind_vmg = Vmg {
            twa: 0.0,
            sail: position::Sail::from_index(0),
            vmg: Default::default()
        };

        let mut downwind_vmg = Vmg {
            twa: 180.0,
            sail: position::Sail::from_index(0),
            vmg: Default::default()
        };

        let tws_indices = Self::interpolation_index(&self.tws, wind_speed.kts());

        for twa in 0..1801 {
            let twa = twa as f64 / 10.0;

            let (max_boat_speed, best_sail, _) = self.get_boat_speed_from_wind_index(wind_speed, using_sail, is_in_ice_limits, tws_indices, twa);

            let vmg = Speed::from_kts(max_boat_speed.kts() * (twa.to_radians().cos()));

            if vmg > upwind_vmg.vmg {
                upwind_vmg.twa = twa;
                upwind_vmg.sail = best_sail.clone();
                upwind_vmg.vmg = vmg.clone();
            }
            if vmg <= downwind_vmg.vmg.clone() {
                downwind_vmg.twa = twa;
                downwind_vmg.sail = best_sail;
                downwind_vmg.vmg = vmg;
            }
        }

        // try to optim vmg
        let mut optimized_upwind_vmg = None;
        let upwind_vmg_twa = upwind_vmg.twa.clone();
        let upwind_vmg_vmg = upwind_vmg.vmg.clone();
        let mut max_boat_speed = Speed::from_kts(0.0);
        for delta_twa in -10..10 {
            let twa = upwind_vmg_twa.round() - (delta_twa as f64 / 10.0);

            let (boat_speed, sail, _) = self.get_boat_speed_from_wind_index(wind_speed, Some(&upwind_vmg.sail), is_in_ice_limits, tws_indices, twa);
            let vmg = Speed::from_kts(boat_speed.kts() * (twa.to_radians().cos()));

            if vmg.kts() >= upwind_vmg_vmg.kts() - 0.001 && boat_speed > max_boat_speed {
                max_boat_speed = boat_speed;
                optimized_upwind_vmg = Some(Vmg {
                    twa,
                    sail,
                    vmg
                });
            }
        }

        let mut optimized_downwind_vmg = None;
        let downwind_vmg_twa = downwind_vmg.twa.clone();
        let downwind_vmg_vmg = downwind_vmg.vmg.clone();
        let mut max_boat_speed = Speed::from_kts(0.0);
        for delta_twa in -10..10 {
            let twa = downwind_vmg_twa.round() + (delta_twa as f64 / 10.0);

            let (boat_speed, sail, _) = self.get_boat_speed_from_wind_index(wind_speed, Some(&downwind_vmg.sail), is_in_ice_limits, tws_indices, twa);
            let vmg = Speed::from_kts(boat_speed.kts() * (twa.to_radians().cos()));

            if vmg.kts() >= downwind_vmg_vmg.kts() - 0.001 && boat_speed > max_boat_speed {
                max_boat_speed = boat_speed;
                optimized_downwind_vmg = Some(Vmg {
                    twa,
                    sail,
                    vmg
                });
            }
        }

        Vmgs {
            up: upwind_vmg,
            optimized_up: optimized_upwind_vmg,
            down: downwind_vmg,
            optimized_down: optimized_downwind_vmg
        }
    }

    fn foil_amount(&self, twa: f64, wind_speed: &Speed) -> f64 {
        let ws = wind_speed.kts();

        let ct = if twa <= self.foil.twa_min-self.foil.twa_merge {
            return 1.0;
        } else if twa < self.foil.twa_min {
            (twa-(self.foil.twa_min-self.foil.twa_merge)) / self.foil.twa_merge
        } else if twa < self.foil.twa_max {
            1.0
        } else if twa < self.foil.twa_max+self.foil.twa_merge {
            (self.foil.twa_max+self.foil.twa_merge-twa) / self.foil.twa_merge
        } else {
            return 1.0;
        };

        let cv = if ws <= self.foil.tws_min-self.foil.tws_merge {
            return 1.0;
        } else if ws < self.foil.tws_min {
            (ws - (self.foil.tws_min - self.foil.tws_merge)) / self.foil.tws_merge
        } else if ws < self.foil.tws_max {
            1.0
        } else if ws < self.foil.tws_max+self.foil.tws_merge {
            (self.foil.tws_max + self.foil.tws_merge - ws) / self.foil.tws_merge
        } else {
            1.0
        };

        1.0 + (self.foil.speed_ratio - 1.0) * ct * cv
    }

    fn get_penalty_values(&self, boat_options: &Arc<BoatOptions>, penalty_case: &PenaltyCase, wind_speed: &Speed, stamina: f64) -> Penalty {

        let stamina_coef = match boat_options.stamina {
            false => 1.0,
            true => {
                0.5 + (100.0 - stamina) / 100.0 * 1.5
            }
        };

        let (lws, hws, bnd) = match (boat_options.winch, self.winch.lws, self.winch.hws, &penalty_case.std, &penalty_case.pro) {
            (true,  Some(lws), Some(hws), _, Some(pro)) => {
                (lws as f64, hws as f64, pro)
            }
            (false, Some(lws), Some(hws), Some(std), _) => {
                (lws as f64, hws as f64, std)
            }
            (true, _, _, _, _) => {
                return Penalty { duration: Duration::seconds((penalty_case.pro_timer_sec as f64 * stamina_coef) as i64), ratio: penalty_case.pro_ratio }
            }
            (false, _, _, _, _) => {
                return Penalty { duration: Duration::seconds((penalty_case.std_timer_sec as f64 * stamina_coef) as i64), ratio: penalty_case.std_ratio }
            }
        };

        if wind_speed.kts() <= lws {
            Penalty { duration: Duration::seconds((bnd.lw.timer as f64 * stamina_coef) as i64), ratio: bnd.lw.ratio }
        } else if wind_speed.kts() >= hws {
            Penalty { duration: Duration::seconds((bnd.hw.timer as f64 * stamina_coef) as i64), ratio: bnd.hw.ratio }
        } else {
            let duration_seconds = Self::interpolation(lws, hws, bnd.lw.timer as f64, bnd.hw.timer as f64, wind_speed.kts());
            let ratio = Self::interpolation(lws, hws, bnd.lw.ratio, bnd.hw.ratio, wind_speed.kts());
            Penalty { duration: Duration::seconds((duration_seconds * stamina_coef) as i64), ratio }
        }
    }

    fn interpolation(x1: f64, x2: f64, y1: f64, y2: f64, x: f64) -> f64 {
        let t = (x - x1) / (x2 - x1);
        (1.0 - t) * ((1.0 - t) * ((1.0 - t) * y1 + t * y1) + t * ((1.0 - t) * y1 + t * y2)) + t * ((1.0 - t) * ((1.0 - t) * y1 + t * y2) + t * ((1.0 - t) * y2 + t * y2))
    }

    pub(crate) fn tired(&self, stamina: f64, previous_twa: f64, new_twa: f64, previous_sail: &position::Sail, new_sail: &position::Sail, wind_speed: &Speed) -> f64 {
        let mut stamina = stamina;

        let stamina_coef = if wind_speed.kts() <= 10.0 {
            1.0 + wind_speed.kts() / 10.0 * 0.25
        } else if wind_speed.kts() <= 20.0 {
            1.25 + (wind_speed.kts() - 10.0) / 10.0 * 0.25
        } else if wind_speed.kts() <= 30.0 {
            1.5 + (wind_speed.kts() - 20.0) / 10.0 * 0.5
        } else {
            2.0
        };

        if previous_twa * new_twa < 0.0 && new_twa.abs() <= 90.0 {
            stamina = stamina - 10.0 * stamina_coef;
        } else if previous_twa * new_twa < 0.0 && new_twa.abs() > 90.0 {
            stamina = stamina - 10.0 * stamina_coef;
        }

        if previous_sail != new_sail {
            stamina = stamina - 20.0 * stamina_coef;
        }

        stamina = stamina.max(0.0);

        stamina
    }

    pub(crate) fn recovers(&self, stamina: f64, duration: &Duration, wind_speed: &Speed) -> f64 {
        let mut stamina = stamina;

        let recovery_time = if wind_speed.kts() <= 0.0 {
            5.0
        } else if wind_speed.kts() >= 30.0 {
            15.0
        } else {
            Self::interpolation(0.0, 30.0, 5.0, 15.0, wind_speed.kts())
        };

        let recovery = duration.num_minutes() as f64 / recovery_time;
        stamina = stamina + recovery;
        stamina = stamina.min(100.0);

        stamina
    }

    pub(crate) fn add_penalties(&self, boat_options: &Arc<BoatOptions>, penalties: Penalties, stamina: f64, previous_twa: f64, new_twa: f64, previous_sail: &position::Sail, new_sail: &position::Sail, wind_speed: &Speed) -> Penalties {
        let mut penalties = penalties;

        if previous_twa * new_twa < 0.0 && new_twa.abs() <= 90.0 {
            penalties.tack = Some(self.get_penalty_values(boat_options, &self.winch.tack, wind_speed, stamina));
        } else if previous_twa * new_twa < 0.0 && new_twa.abs() > 90.0 {
            penalties.gybe = Some(self.get_penalty_values(boat_options, &self.winch.gybe, wind_speed, stamina));
        }

        if previous_sail != new_sail {
            penalties.sail_change = Some(self.get_penalty_values(boat_options, &self.winch.sail_change, wind_speed, stamina));
        }

        penalties
    }

    pub(crate) fn distance(boat_speed: Speed, duration: Duration, penalties: &Penalties) -> (Distance, Penalties, Speed, f64) {

        if duration.is_zero() {
            return (Distance::from_m(0.0), penalties.clone(), boat_speed, 1.0);
        }

        if !penalties.is_some() {
            return (boat_speed.clone() * duration, penalties.clone(), boat_speed, 1.0)
        }

        if let Some(penalty_duration) = penalties.min_penalty_duration() {
            let penalty_duration = penalty_duration.min(duration);
            let (penalties, ratio) = penalties.navigate(penalty_duration);

            let (dist, penalties, _, _) = Self::distance(boat_speed.clone(), duration - penalty_duration, &penalties);

            let boat_speed = boat_speed * ratio;
            (boat_speed.clone() * penalty_duration + dist, penalties, boat_speed, ratio)

        } else {
            (boat_speed.clone() * duration, penalties.clone(), boat_speed, 1.0)
        }
    }

    pub(crate) fn duration(boat_speed: Speed, distance: Distance, penalties: Penalties) -> (Duration, Penalties, Speed, f64) {

        let penalties_vec = penalties.to_vec();

        if penalties_vec.len() > 0 {

            let new_boat_speed = boat_speed.clone() * penalties_vec[0].ratio;

            // if remaining distance < the one we can
            if distance <= new_boat_speed.clone() * penalties_vec[0].duration {
                let duration = distance / new_boat_speed.clone();

                return (duration, penalties - duration, new_boat_speed, penalties_vec[0].ratio);
            } else {
                let (duration, penalties, _, _) = Self::duration(boat_speed, distance - &(new_boat_speed.clone() * penalties_vec[0].duration), penalties - penalties_vec[0].duration);

                return (penalties_vec[0].duration + duration, penalties, new_boat_speed, penalties_vec[0].ratio);
            }
        }

        (distance / boat_speed.clone(), penalties, boat_speed, 1.0)
    }

}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Vmgs {
    pub(crate) up: Vmg,
    pub(crate) optimized_up: Option<Vmg>,
    pub(crate) down: Vmg,
    pub(crate) optimized_down: Option<Vmg>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Vmg {
    pub(crate) twa: f64,
    pub(crate) sail: position::Sail,
    pub(crate) vmg: Speed,
}

#[derive(Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Polar {
    #[serde(rename = "_id")]
    pub(crate) id: u8,
    pub(crate) label: String,
    pub(crate) global_speed_ratio: f64,
    pub(crate) ice_speed_ratio: f64,
    pub(crate) auto_sail_change_tolerance: f64,
    pub(crate) bad_sail_tolerance: f64,
    pub(crate) max_speed: f64,
    pub(crate) foil: Foil,
    pub(crate) hull: Hull,
    pub(crate) winch: Winch,
    pub(crate) tws: Vec<f64>,
    pub(crate) twa: Vec<f64>,
    pub(crate) sail: Vec<PolarSail>,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Foil {
    pub(crate) speed_ratio: f64,
    pub(crate) twa_min: f64,
    pub(crate) twa_max: f64,
    pub(crate) twa_merge: f64,
    pub(crate) tws_min: f64,
    pub(crate) tws_max: f64,
    pub(crate) tws_merge: f64,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Hull {
    pub(crate) speed_ratio: f64,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Winch {
    pub(crate) tack: PenaltyCase,
    pub(crate) gybe: PenaltyCase,
    pub(crate) sail_change: PenaltyCase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lws: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hws: Option<u8>,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PenaltyCase {
    pub(crate) std_timer_sec: u16,
    pub(crate) std_ratio: f64,
    pub(crate) pro_timer_sec: u16,
    pub(crate) pro_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) std: Option<PenaltyBoundaries>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pro: Option<PenaltyBoundaries>,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PenaltyBoundaries {
    pub(crate) lw: PolarPenalty,
    pub(crate) hw: PolarPenalty,
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolarPenalty {
    pub(crate) ratio: f64,
    pub(crate) timer: u16
}

#[derive(Deserialize, Serialize, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolarSail {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) speed: Vec<Vec<f64>>
}