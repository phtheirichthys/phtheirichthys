use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::{fmt, ops};
use chrono::Duration;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Visitor;
use tsify_next::Tsify;

#[derive(Clone, Debug, Default, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Speed {
    pub(crate) value: f64,
    pub(crate) unit: SpeedUnit,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum SpeedUnit {
    #[default]
    Knot,
    MeterPerSecond,
    KiloMeterPerHour,
}

impl Speed {

    pub(crate) const MIN: Speed = Speed {
        value: 2.0,
        unit: SpeedUnit::Knot,
    };

    pub fn from_kts(value: f64) -> Self {
        Self {
            value,
            unit: SpeedUnit::Knot
        }
    }

    pub(crate) fn from_m_s(value: f64) -> Self {
        Self {
            value,
            unit: SpeedUnit::MeterPerSecond
        }
    }

    pub(crate) fn from_km_h(value: f64) -> Self {
        Self {
            value,
            unit: SpeedUnit::KiloMeterPerHour
        }
    }

    pub(crate) fn kts(&self) -> f64 {
        match &self.unit {
            SpeedUnit::Knot => self.value,
            SpeedUnit::MeterPerSecond => self.value * 3.6 / 1.852,
            SpeedUnit::KiloMeterPerHour => self.value / 1.852,
        }
    }

    pub(crate) fn m_s(&self) -> f64 {
        match &self.unit {
            SpeedUnit::Knot => self.value * 1.852 / 3.6,
            SpeedUnit::MeterPerSecond => self.value,
            SpeedUnit::KiloMeterPerHour => self.value / 3.6,
        }
    }

    pub(crate) fn km_h(&self) -> f64 {
        match &self.unit {
            SpeedUnit::Knot => self.value * 1.852,
            SpeedUnit::MeterPerSecond => self.value * 3.6,
            SpeedUnit::KiloMeterPerHour => self.value,
        }
    }

    pub(crate) fn gap(&self, other: &Self) -> u8 {
        ((self.kts() - other.kts()) / self.kts()).abs() as u8
    }
}

impl Display for Speed {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match &self.unit {
      SpeedUnit::Knot => write!(f, "{}kt (kt)", self.kts()),
      SpeedUnit::MeterPerSecond => write!(f, "{}kt (m/s)", self.kts()),
      SpeedUnit::KiloMeterPerHour => write!(f, "{}kt (km/h)", self.kts()),
    }
  }
}

impl PartialOrd<Self> for Speed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.kts().partial_cmp(&other.kts())
    }
}

impl PartialEq<Self> for Speed {
    fn eq(&self, other: &Self) -> bool {
        self.kts().eq(&other.kts())
    }
}

impl ops::MulAssign<f64> for Speed {
    fn mul_assign(&mut self, rhs: f64) {
        self.value *= rhs
    }
}

impl ops::Mul<f64> for Speed {
    type Output = Speed;

    fn mul(self, rhs: f64) -> Self::Output {
        Speed::from_m_s(self.m_s() * rhs)
    }
}

impl ops::Mul<Duration> for Speed {
    type Output = Distance;

    fn mul(self, rhs: Duration) -> Self::Output {
        Distance {
            value: self.m_s() * rhs.num_seconds() as f64,
            unit:DistanceUnit::Meters,
        }
    }
}

impl Serialize for Speed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.kts())
    }
}

struct SpeedVisitor;

impl<'de> Visitor<'de> for SpeedVisitor {
    type Value = Speed;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between -2^31 and 2^31")
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Speed::from_kts(value))
    }

    fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Speed::from_kts(value as f64))
    }

    fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Speed::from_kts(value as f64))
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Speed::from_kts(value as f64))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Speed::from_kts(value as f64))
    }

}

impl<'de> Deserialize<'de> for Speed {
    fn deserialize<D>(deserializer: D) -> Result<Speed, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_f64(SpeedVisitor)
    }
}

#[derive(Clone, Debug, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Distance {
    pub value: f64,
    pub unit: DistanceUnit,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum DistanceUnit {
    Meters,
    #[default]
    NauticalMiles,
}

impl Distance {
    pub(crate) fn zero() -> Self {
        Distance {
            value: 0.0,
            unit: DistanceUnit::Meters
        }
    }

    pub(crate) fn _is_zero(&self) -> bool {
        self.value == 0.0
    }

    pub(crate) fn from_m(value: f64) -> Self {
        Distance {
            value,
            unit: DistanceUnit::Meters
        }
    }

    pub(crate) fn from_nm(value: f64) -> Self {
        Distance {
            value,
            unit: DistanceUnit::NauticalMiles
        }
    }

    pub(crate) fn m(&self) -> f64 {
        match &self.unit {
            DistanceUnit::Meters => self.value,
            DistanceUnit::NauticalMiles => self.value * 1852.0,
        }
    }

    pub(crate) fn nm(&self) -> f64 {
        match &self.unit {
            DistanceUnit::Meters => self.value / 1852.0,
            DistanceUnit::NauticalMiles => self.value,
        }
    }

    fn val(&self, unit: &DistanceUnit) -> f64 {
        match unit {
            DistanceUnit::Meters => self.m(),
            DistanceUnit::NauticalMiles => self.nm(),
        }
    }
}

impl Display for Distance {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match &self.unit {
      DistanceUnit::Meters => write!(f, "{}m", self.value),
      DistanceUnit::NauticalMiles => write!(f, "{}nm", self.value),
    }
  }
}

impl PartialEq<Self> for Distance {
    fn eq(&self, other: &Self) -> bool {
        self.m().eq(&other.m())
    }
}

impl Eq for Distance {}

impl PartialOrd<Self> for Distance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Distance {
    fn cmp(&self, other: &Self) -> Ordering {
        self.m().total_cmp(&other.m())
    }
}

impl PartialEq<&Distance> for Distance {
    fn eq(&self, other: &&Distance) -> bool {
        self.m().eq(&other.m())
    }
}

impl PartialOrd<&Distance> for Distance {
    fn partial_cmp(&self, other: &&Distance) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

impl ops::Mul<f64> for Distance {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Distance {
            value: self.value * rhs,
            unit: self.unit
        }
    }
}

impl ops::Mul<Distance> for Distance {
    type Output = Self;

    fn mul(self, rhs: Distance) -> Self {
        Distance {
            value: self.value * rhs.val(&self.unit),
            unit: self.unit
        }
    }
}

impl ops::Add<Distance> for Distance {
    type Output = Self;

    fn add(self, rhs: Distance) -> Self {
        Distance {
            value: self.value + rhs.val(&self.unit),
            unit: self.unit
        }
    }
}

impl ops::Add<&Distance> for Distance {
    type Output = Self;

    fn add(self, rhs: &Distance) -> Self {
        Distance {
            value: self.value + rhs.val(&self.unit),
            unit: self.unit
        }
    }
}

impl ops::Sub<&Distance> for Distance {
    type Output = Self;

    fn sub(self, rhs: &Distance) -> Self {
        Distance {
            value: self.value - rhs.val(&self.unit),
            unit: self.unit
        }
    }
}

impl ops::Div<f64> for Distance {
    type Output = Self;

    fn div(self, rhs: f64) -> Self {
        Distance {
            value: self.value / rhs,
            unit: self.unit
        }
    }
}

impl ops::Div<Distance> for Distance {
    type Output = Self;

    fn div(self, rhs: Distance) -> Self {
        Distance {
            value: self.value / rhs.val(&self.unit),
            unit: self.unit.clone()
        }
    }
}

impl ops::Div<Speed> for Distance {
    type Output = Duration;

    fn div(self, rhs: Speed) -> Duration {
        if rhs.m_s() == 0.0 {
            Duration::max_value()
        } else {
            Duration::seconds((self.m() / rhs.m_s()) as i64)
        }
    }
}


impl Serialize for Distance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.nm())
    }
}

struct DistanceVisitor;

impl<'de> Visitor<'de> for DistanceVisitor {
    type Value = Distance;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between -2^31 and 2^31")
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        Ok(Distance::from_nm(value))
    }

    fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Distance::from_nm(value as f64))
    }

}

impl<'de> Deserialize<'de> for Distance {
    fn deserialize<D>(deserializer: D) -> Result<Distance, D::Error>
        where
            D: Deserializer<'de>,
    {
        deserializer.deserialize_f64(DistanceVisitor)
    }
}
