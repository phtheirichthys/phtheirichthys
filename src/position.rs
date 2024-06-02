use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Sub;
use chrono::Duration;
use serde::{Serialize, Deserialize, Serializer, de, Deserializer};
use serde::de::Visitor;
use tsify::Tsify;
use wasm_bindgen::prelude::*;
use crate::polar::Vmgs;
use crate::router;
use crate::utils::Speed;
use crate::wind::Wind;


#[derive(Clone, Default, Debug, Serialize, Tsify, Deserialize, PartialEq)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Coords {
    pub(crate) lat: f64,
    pub(crate) lon: f64,
}

impl From<(f64, f64)> for Coords {
    fn from(latlon: (f64, f64)) -> Self {
        Coords {
            lat: latlon.0,
            lon: latlon.1,
        }
    }
}

impl From<[f64; 2]> for Coords {
    fn from(latlon: [f64; 2]) -> Self {
        Coords {
            lat: latlon[0],
            lon: latlon[1],
        }
    }
}

impl Display for Coords {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.lat, self.lon)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    pub(crate) heading: Heading,
    pub(crate) sail: Sail,
}

impl PartialEq<Settings> for Settings {
    fn eq(&self, other: &Settings) -> bool {
        self.sail == other.sail && self.heading == other.heading
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) struct Sail {
    pub(crate) index: usize,
    pub(crate) id: usize,
    pub(crate) auto: bool,
}

impl Sail {
    pub const AUTO: Sail = Sail {
        index: 0,
        id: 1,
        auto: true
    };

    pub(crate) fn from_index(sail: usize) -> Self {
        Self {
            index: sail,
            id: sail + 1,
            auto: false
        }
    }
}

impl PartialEq<Sail> for Sail {
    fn eq(&self, other: &Sail) -> bool {
        self.id == other.id
    }
}

impl From<usize> for Sail {
    fn from(sail_id: usize) -> Self {
        Self {
            index: (sail_id % 10).max(1) - 1,
            id: (sail_id % 10).max(1),
            auto: sail_id >= 10
        }
    }
}

impl Into<usize> for Sail {
    fn into(self) -> usize {
        if self.auto {
            10
        } else {
            self.id
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Status {
    pub(crate) aground: bool,
    pub(crate) boat_speed: Speed,
    pub(crate) wind: Wind,
    pub(crate) foil: u8,
    pub(crate) boost: u8,
    pub(crate) best_ratio: f64,
    pub(crate) ratio: u8,
    pub(crate) vmgs: Option<Vmgs>,
    pub(crate) penalties: Penalties,
    pub(crate) stamina: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Penalties {
    pub(crate) gybe: Option<Penalty>,
    pub(crate) sail_change: Option<Penalty>,
    pub(crate) tack: Option<Penalty>,
}

impl Into<Vec<router::Penalty>> for Penalties {
    fn into(self) -> Vec<router::Penalty> {

        let mut res = Vec::new();

        self.gybe.map(|g| if g.duration > Duration::zero() {
            res.push(router::Penalty {
                duration: g.duration.clone(),
                ratio: g.ratio,
                typ: 1
            });
        });

        self.tack.map(|g| if g.duration > Duration::zero() {
            res.push(router::Penalty {
                duration: g.duration.clone(),
                ratio: g.ratio,
                typ: 2
            });
        });

        self.sail_change.map(|g| if g.duration > Duration::zero() {
            res.push(router::Penalty {
                duration: g.duration.clone(),
                ratio: g.ratio,
                typ: 4
            });
        });

        res
    }
}

impl Penalties {
    pub(crate) fn new() -> Self {
        Self {
            gybe: None,
            sail_change: None,
            tack: None,
        }
    }

    pub(crate) fn is_some(&self) -> bool {
        self.gybe.as_ref().is_some_and(|p| !p.duration.is_zero()) || self.sail_change.as_ref().is_some_and(|p| !p.duration.is_zero()) || self.tack.as_ref().is_some_and(|p| !p.duration.is_zero())
    }

    pub(crate) fn min_penalty_duration(&self) -> Option<Duration> {

        let mut min = None;

        self.gybe.as_ref().map(|gybe| {
            if !min.as_ref().is_some_and(|min| min <= &gybe.duration) {
                min.replace(gybe.duration.clone());
            }
        });

        self.sail_change.as_ref().map(|sail_change| {
            if !min.as_ref().is_some_and(|min| min <= &sail_change.duration) {
                min.replace(sail_change.duration.clone());
            }
        });

        self.tack.as_ref().map(|tack| {
            if !min.as_ref().is_some_and(|min| min <= &tack.duration) {
                min.replace(tack.duration.clone());
            }
        });

        min
    }

    pub(crate) fn duration(&self) -> Duration {
        self.gybe.as_ref().map_or(Duration::zero(), |p| p.duration.clone()).max(
            self.sail_change.as_ref().map_or(Duration::zero(), |p| p.duration.clone()).max(
            self.tack.as_ref().map_or(Duration::zero(), |p| p.duration.clone())))
    }

    pub(crate) fn navigate(&self, duration: Duration) -> (Self, f64) {

        let mut ratio = 1.0;

        (Self {
            gybe: self.gybe.as_ref().and_then(|gybe| { ratio *= gybe.ratio; if gybe.duration <= duration { None } else { Some(Penalty { duration: gybe.duration - duration, ratio: gybe.ratio })}}),
            sail_change: self.sail_change.as_ref().and_then(|sail_change| { ratio *= sail_change.ratio; if sail_change.duration <= duration { None } else { Some(Penalty { duration: sail_change.duration - duration, ratio: sail_change.ratio })}}),
            tack: self.tack.as_ref().and_then(|tack| { ratio *= tack.ratio; if tack.duration <= duration { None } else { Some(Penalty { duration: tack.duration - duration, ratio: tack.ratio })}}),
        }, ratio)
    }

    pub(crate) fn to_vec(&self) -> Vec<Penalty> {

        let penalties = Self::merge_penalty(Vec::new(), 0, self.gybe.clone());
        let penalties = Self::merge_penalty(penalties, 0, self.sail_change.clone());
        let penalties = Self::merge_penalty(penalties, 0, self.tack.clone());

        penalties
    }

    fn merge_penalty(penalties: Vec<Penalty>, index: usize, penalty: Option<Penalty>) -> Vec<Penalty> {

        let mut penalties = penalties;

        if let Some(penalty) = penalty {

            if penalty.duration.is_zero() {
                return penalties;
            }

            if penalties.len() == 0 || penalties.len() >= index {

                penalties.push(penalty);
            } else if penalties[index].duration <= penalty.duration {

                penalties[index].ratio *= penalty.ratio;
                let new_penalty = Penalty { duration: penalty.duration - penalties[index].duration.clone(), ratio: penalty.ratio };
                penalties = Self::merge_penalty(penalties, index + 1, Some(new_penalty))
            } else {

                penalties[index].duration = penalties[index].duration - penalty.duration;
                let penalty = Penalty { duration: penalties[index].duration - penalty.duration.clone(), ratio: penalty.ratio * penalties[index].ratio };
                penalties.insert(index, penalty);
            }
        }

        penalties
    }

    pub(crate) fn _total(&self) -> Duration {
        self.gybe.as_ref().map_or(Duration::zero(), |p| p.duration.clone()) + self.sail_change.as_ref().map_or(Duration::zero(), |p| p.duration.clone()) + self.tack.as_ref().map_or(Duration::zero(), |p| p.duration.clone())
    }
}

impl Sub<Duration> for Penalties {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {

        Self {
            gybe: self.gybe.and_then(|p| if p.duration <= rhs { None } else { Some(Penalty { duration: p.duration - rhs, ratio: p.ratio }) } ),
            sail_change: self.sail_change.and_then(|p| if p.duration <= rhs { None } else { Some(Penalty { duration: p.duration - rhs, ratio: p.ratio }) } ),
            tack: self.tack.and_then(|p| if p.duration <= rhs { None } else { Some(Penalty { duration: p.duration - rhs, ratio: p.ratio }) } )
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Penalty {
    #[serde(serialize_with = "duration_to_seconds", deserialize_with = "seconds_to_duration")]
    pub(crate) duration: Duration,
    pub(crate) ratio: f64,
}

fn duration_to_seconds<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
    serializer.serialize_i64(duration.num_seconds())
}

struct DurationVisitor;

impl<'de> Visitor<'de> for DurationVisitor {
    type Value = Duration;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between -2^31 and 2^31")
    }

    fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value as i64))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Duration::seconds(value))
    }
}

fn seconds_to_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
{
    deserializer.deserialize_i64(DurationVisitor)
}

#[derive(Clone, Debug, Deserialize, Serialize, Tsify)]
#[serde(rename_all = "lowercase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub(crate) enum Heading {
    HEADING(f64),
    TWA(f64),
}

impl Heading {

    pub(crate) fn is_regulated(&self) -> bool {
        match self {
            Heading::HEADING(_) => false,
            Heading::TWA(_) => true
        }
    }

    pub(crate) fn heading(&self, twd: f64) -> f64 {
        match self {
            Heading::HEADING(heading) => heading.clone(),
            Heading::TWA(twa) => {
                let mut heading = twd - twa;
                if heading < 0.0 {
                    heading += 360.0
                }
                if heading >= 360.0 {
                    heading -= 360.0
                }

                heading
            }
        }
    }

    pub(crate) fn twa(&self, twd: f64) -> f64 {
        match self {
            Heading::HEADING(heading) => {
                let mut twa = twd - heading;
                if twa <= -180.0 {
                    twa += 360.0
                }
                if twa > 180.0 {
                    twa -= 360.0
                }

                twa
            },
            Heading::TWA(twa) => twa.clone(),
        }
    }
}

impl Default for Heading {
    fn default() -> Self {
        Heading::TWA(0.0)
    }
}

impl PartialEq<Heading> for Heading {
    fn eq(&self, other: &Heading) -> bool {
        match (self, other) {
            (Heading::HEADING(h), Heading::HEADING(o)) => {
                h.eq(o)
            }
            (Heading::TWA(t), Heading::TWA(o)) => {
                t.eq(o)
            }
            _ => false
        }
    }
}

impl Display for Heading {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Heading::HEADING(heading) => {
                write!(f, "heading {}", heading)
            }
            Heading::TWA(twa) => {
                write!(f, "regulated twa {}", twa)
            }
        }
    }
}

impl Display for Sail {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let sails = ["Jib", "Spi", "Staysail", "LightJib", "Code0", "HeavyGnk", "LightGnk"];
        write!(f, "{}{}", sails[self.index], if self.auto {"*"} else {""})
    }
}
