use std::{fmt::{Display, Formatter}, sync::Arc};

use chrono::{DateTime, Duration, DurationRound, Utc};

use super::Wind;

pub(crate) type RefTime = DateTime<Utc>;

impl RefTimeSpec for RefTime {}

pub(crate) trait RefTimeSpec {
    fn new(time: DateTime<Utc>) -> RefTime {
        time.duration_trunc(6.hours()).expect("now truncated by 6 hours")
    }

    fn now() -> RefTime {
        Self::new(Utc::now())
    }
}

pub type ForecastTime = DateTime<Utc>;

pub(crate) trait ForecastTimeSpec {
    fn from_ref_time(ref_time: &RefTime, h: u16) -> ForecastTime {
        *ref_time + h.hours()
    }

    fn from_now(&self) -> Duration;
}

impl ForecastTimeSpec for ForecastTime {
    fn from_now(&self) -> Duration {
        *self - Utc::now()
    }
}

impl Durations for u16 {
    fn hours(&self) -> Duration {
        chrono::Duration::hours(*self as i64)
    }
}

pub(crate) trait Durations {
    fn hours(&self) -> chrono::Duration;
}


pub struct Stamp {
    pub(crate) id: StampId,
    pub(crate) wind: Arc<Wind>,
}

#[derive(Clone)]
pub struct StampId {
    pub ref_time: RefTime,
    pub forecast_time: ForecastTime,
}

impl StampId {
    pub(crate) fn new(ref_time: &RefTime, forecast_time: ForecastTime) -> Self {
        StampId {
            ref_time: ref_time.clone(),
            forecast_time
        }
    }

    pub(crate) fn from_now(&self) -> Duration {
        self.forecast_time - Utc::now()
    }

    pub(crate) fn forecast_hour(&self) -> u16 {
        (self.forecast_time - self.ref_time).num_hours() as u16
    }

    pub(crate) fn file_name(&self) -> String {
        format!("{}.f{:03}", self.ref_time.format("%Y%m%d%H"), self.forecast_hour())
    }
}

impl From<(&RefTime, ForecastTime)> for StampId {
    fn from((ref_time, forecast_time): (&RefTime, ForecastTime)) -> Self {
        Self {
            ref_time: ref_time.clone(),
            forecast_time,
        }
    }
}

impl From<(&RefTime, u16)> for StampId {
    fn from((ref_time, h): (&RefTime, u16)) -> Self {
        Self {
            ref_time: ref_time.clone(),
            forecast_time: *ref_time + Duration::hours(h as i64),
        }
    }
}

impl Stamp {
    pub(crate) fn from_now(&self) -> Duration {
        self.id.from_now()
    }

    pub(crate) fn forecast_hour(&self) -> u16 {
        self.id.forecast_hour()
    }

    pub(crate) fn file_name(&self) -> String {
        self.id.file_name()
    }
}

impl Display for StampId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}Z+{:03}", self.ref_time.format("%H"), self.forecast_hour())
    }
}
