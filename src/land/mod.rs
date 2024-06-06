use log::debug;

use crate::position::Coords;

pub(crate) mod vr;

pub(crate) trait LandsProvider {
    fn is_land(&self, lat: f64, lon: f64) -> bool;

    fn is_next_land(&self, lat: f64, lon: f64) -> bool {
        for i in -1..2 {
            for j in -1..2 {
                let lat = lat + (i as f64) / (730.0 / 2.0);
                let lon = lon + (j as f64) / (730.0 / 2.0);

                if self.is_land(lat, lon) {
                    return true
                }
            }
        }

        return false
    }

    fn _cross_land(&self, from: &Coords, to: &Coords) -> bool {

        const STEP: i8 = 10;

        for i in 0..(STEP + 1) {
            let lat = from.lat + (i as f64) * (to.lat - from.lat) / (STEP as f64);
            let lon = from.lon + (i as f64) * (to.lon - from.lon) / (STEP as f64);
            if self.is_land(lat, lon) {
                return true;
            }
        }

        false
    }

    fn cross_next_land(&self, from: &Coords, to: &Coords) -> bool {

        let next = self.is_next_land(from.lat, from.lon);

        const STEP: i8 = 10;

        for i in 0..(STEP + 1) {
            let lat = from.lat + (i as f64) * (to.lat - from.lat) / (STEP as f64);
            let lon = from.lon + (i as f64) * (to.lon - from.lon) / (STEP as f64);
            if next && self.is_land(lat, lon) || !next && self.is_next_land(lat, lon) {
                return true;
            }
        }

        false
    }

    fn _best_to_leave(&self, from: &Coords) -> f64 {

        let deltas = [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0)];
        let headings = [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

        let distances = [
            [0, 1, 2, 3, 4, 3, 2, 1],
            [1, 0, 1, 2, 3, 4, 3, 2],
            [2, 1, 0, 1, 2, 3, 4, 3],
            [3, 2, 1, 0, 1, 2, 3, 4],
            [4, 3, 2, 1, 0, 1, 2, 3],
            [3, 4, 3, 2, 1, 0, 1, 2],
            [2, 3, 4, 3, 2, 1, 0, 1],
            [1, 2, 3, 4, 3, 2, 1, 0],
        ];

        let mut lands = [false;8];
        let mut scores = [0;8];

        for i in 0..8 {
            let lat = from.lat + deltas[i].0 * 0.7/730.0;
            let lon = from.lon + deltas[i].1 * 0.7/730.0;
            lands[i] = self.is_land(lat, lon);
        }

        debug!("lands : {:?}", lands);

        for i in 0..8 {
            scores[i] = distances[i].iter().enumerate()
                .filter(|(o, _)| lands[*o])
                .min_by(|(_, a), (_, b)| a.cmp(b))
                .map_or(0, |(_, d)| *d);
        }

        debug!("scores : {:?}", scores);

        headings.iter().enumerate().max_by(|(a, _), (b, _)| scores[*a].cmp(&scores[*b])).map(|(_, heading)| *heading).unwrap()
    }

    fn near_land(&self, lat: f64, lon: f64) -> bool {
        for i in -2..3 {
            for j in -2..3 {
                if self.is_land(lat + (i as f64) / 730.0, lon + (j as f64) / 730.0) {
                    return true
                }
            }
        }

        false
    }

}