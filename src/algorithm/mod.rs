use crate::position::Point;
use crate::utils::{Distance, DistanceUnit};

pub(crate) mod spherical;

const MEAN_EARTH_RADIUS: Distance = Distance {
    value: 6371008.8,
    unit: DistanceUnit::Meters,
};

pub(crate) trait Algorithm {
    fn distance_to(&self, from: &Point, to: &Point) -> Distance;

    fn heading_to(&self, from: &Point, to: &Point) -> f64;

    fn distance_and_heading_to(&self, from: &Point, to: &Point) -> (Distance, f64);

    fn destination(&self, from : &Point, heading: f64, distance: &Distance) -> Point;

    fn intersection(&self, line: (&Point, &Point), from: &Point, heading: f64) -> Option<Point>;
}

trait Utils {
    fn wrap360(self) -> Self;
}

impl Utils for f64 {
    fn wrap360(self) -> Self {
        if 0.0 <= self && self < 360.0 {
            return self.clone()
        }
        let d1 = self + 360.0;
        let d2 = d1 - ((d1/360.0) as i64 * 360) as f64;
        return d2
    }
}