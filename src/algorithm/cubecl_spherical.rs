use cubecl::prelude::*;
use std::f32::consts;
use crate::algorithm::{Algorithm, Distance, Utils};
use crate::position::Coords;


#[cube(launch_unchecked)]
pub(crate) fn distance_to_array<F: Float>(from_lat: &Array<F>, from_lon: &Array<F>, to_lat: &Array<F>, to_lon: &Array<F>, output: &mut Array<F>) {
    if ABSOLUTE_POS < from_lat.len() {
        output[ABSOLUTE_POS] = distance_to_scalar::<F>(from_lat[ABSOLUTE_POS], from_lon[ABSOLUTE_POS], to_lat[ABSOLUTE_POS], to_lon[ABSOLUTE_POS]);
    }
}

#[cube]
fn distance_to_scalar<F: Float>(from_lat: F, from_lon: F, to_lat: F, to_lon: F) -> F {
    let mean_earth_radius = F::new(6371008.8);
    let PI = F::new(3.14159265358979323846264338327950288);
    let TAU = F::new(6.28318530717958647692528676655900577);
    let FRAC_PI_4 = F::new(0.785398163397448309615660845819875721);

    let φ1 = from_lat * PI / F::new(180.0);
    let φ2 = to_lat * PI / F::new(180.0);
    let δφ = φ2 - φ1;

    let mut δλ = (to_lon - from_lon) * PI / F::new(180.0);
    if F::abs(δλ) > PI {
        if δλ > 0.0 {
            δλ = δλ - TAU
        } else {
            δλ = TAU + δλ
        }
    }

    let δψ = F::log(
        (F::sin(φ2/F::new(2.0)+FRAC_PI_4) / (F::cos(φ2/F::new(2.0)+FRAC_PI_4))) /
        (F::sin(φ1/F::new(2.0)+FRAC_PI_4) / F::cos(φ1/F::new(2.0)+FRAC_PI_4))
    );

    //distance
    let mut q = δφ / δψ;
    if F::abs(δψ) <= F::new(10e-12) {
        q = F::cos(φ1)
    }

    let δ = F::sqrt(δφ * δφ + q*q* δλ * δλ);
    let d = mean_earth_radius * δ;

    d
}
