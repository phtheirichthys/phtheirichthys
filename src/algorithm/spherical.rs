use std::f64::consts;
use std::f64::consts::PI;
use crate::algorithm::{Algorithm, Distance, MEAN_EARTH_RADIUS, Utils};
use crate::position::Coords;

pub(crate) struct Spherical {
}

impl Algorithm for Spherical {
    fn distance_to(&self, from: &Coords, to: &Coords) -> Distance {
        let φ1 = from.lat.to_radians();
        let φ2 = to.lat.to_radians();
        let δφ = φ2 - φ1;

        let mut δλ = (to.lon - from.lon).to_radians();
        if δλ.abs() > consts::PI {
            if δλ > 0.0 {
                δλ = -(consts::TAU - δλ)
            } else {
                δλ = consts::TAU + δλ
            }
        }

        let δψ = ((φ2/2.0+consts::FRAC_PI_4).tan() / (φ1/2.0+consts::FRAC_PI_4).tan()).ln();

        //distance
        let mut q = δφ / δψ;
        if δψ.abs() <= 10e-12 {
            q = φ1.cos()
        }

        let δ = (δφ * δφ + q*q* δλ * δλ).sqrt();
        let d = MEAN_EARTH_RADIUS * δ;

        return d
    }

    fn heading_to(&self, from: &Coords, to: &Coords) -> f64 {
        let φ1 = from.lat.to_radians();
        let φ2 = to.lat.to_radians();

        let mut δλ = (to.lon - from.lon).to_radians();
        if δλ.abs() > consts::PI {
            if δλ > 0.0 {
                δλ = -(consts::TAU - δλ)
            } else {
                δλ = consts::TAU + δλ
            }
        }

        let δψ = ((φ2/2.0+consts::FRAC_PI_4).tan() / (φ1/2.0+consts::FRAC_PI_4).tan()).ln();

        //heading
        let θ = δλ.atan2(δψ);

        let b = θ.to_degrees();

        return b.wrap360()
    }

    fn distance_and_heading_to(&self, from: &Coords, to: &Coords) -> (Distance, f64) {
        let φ1 = from.lat.to_radians();
        let φ2 = to.lat.to_radians();
        let δφ = φ2 - φ1;

        let mut δλ = (to.lon - from.lon).to_radians();
        if δλ.abs() > consts::PI {
            if δλ > 0.0 {
                δλ = -(consts::TAU - δλ)
            } else {
                δλ = consts::TAU + δλ
            }
        }

        let δψ = ((φ2/2.0+consts::FRAC_PI_4).tan() / (φ1/2.0+consts::FRAC_PI_4).tan()).ln();

        //distance
        let mut q = δφ / δψ;
        if δψ.abs() <= 10e-12 {
            q = φ1.cos()
        }

        let δ = (δφ * δφ + q*q* δλ * δλ).sqrt();
        let d = MEAN_EARTH_RADIUS * δ;

        //heading
        let θ = δλ.atan2(δψ);

        let b = θ.to_degrees();

        return (d, b.wrap360())
    }

    fn destination(&self, from: &Coords, heading: f64, distance: &Distance) -> Coords {
        let φ1 = from.lat.to_radians();
        let λ1 = from.lon.to_radians();
        let θ = heading.to_radians();

        let δ = distance.m() / MEAN_EARTH_RADIUS.m();

        let δφ = δ * θ.cos();
        let mut φ2 = φ1 + δφ;

        if φ2.abs() > consts::FRAC_PI_2 {
            if φ2 > 0.0 {
                φ2 = consts::PI - φ2
            } else {
                φ2 = -consts::PI - φ2
            }
        }

        let δψ = ((φ2 / 2.0 + consts::FRAC_PI_4).tan() / (φ1 / 2.0 + consts::FRAC_PI_4).tan()).ln();

        let mut q = δφ / δψ;
        if δψ.abs() <= 10e-12 {
            q = φ1.cos();
        }

        let δλ = δ * θ.sin() / q;
        let λ2 = λ1 + δλ;

        Coords {
            lat: φ2.to_degrees(),
            lon: λ2.to_degrees(),
        }
    }

    fn intersection(&self, line: (&Coords, &Coords), p2: &Coords, brng2: f64) -> Option<Coords> {

        // see www.edwilliams.org/avform.htm#Intersection

        let p1 = line.0;
        let brng1 = self.heading_to(line.0, line.1);

        let (φ1, λ1) = (p1.lat.to_radians(), p1.lon.to_radians());
        let (φ2, λ2) = (p2.lat.to_radians(), p2.lon.to_radians());
        let (θ13, θ23) = (brng1.to_radians(), brng2.to_radians());
        let (δφ, δλ) = (φ2 - φ1, λ2 - λ1);

        // angular distance p1-p2
        let δ12 = 2.0 * (((δφ /2.0).sin() * (δφ /2.0).sin() + φ1.cos() * φ2.cos()).sqrt() * (δλ /2.0).sin() * (δλ /2.0).sin()).asin();
        if δ12.abs() < f64::EPSILON {
            return Some(p1.clone()); // coincident points
        }

        // initial/final bearings between points
        let cosθa = (φ2.sin() - φ1.sin()*δ12.cos()) / (δ12.sin()*φ1.cos());
        let cosθb = (φ1.sin() - φ2.sin()*δ12.cos()) / (δ12.sin()*φ2.cos());
        let θa = cosθa.max(-1.0).min(1.0).acos(); // protect against rounding errors
        let θb = cosθb.max(-1.0).min(1.0).acos(); // protect against rounding errors

        let θ12 = if (λ2-λ1).sin() > 0.0 { θa } else { 2.0 * PI - θa } ;
        let θ21 = if (λ2-λ1).sin() > 0.0 { 2.0 * PI - θb } else { θb };

        let a1 = θ13 - θ12; // angle 2-1-3
        let a2 = θ21 - θ23; // angle 1-2-3

        if a1.sin() == 0.0 && a2.sin() == 0.0 // infinite intersections
            || a1.sin() * a2.sin() < 0.0 // ambiguous intersection (antipodal/360°)
        {
            return None;
        }

        let cosα3 = -a1.cos()* a2.cos() + a1.sin()* a2.sin()*δ12.cos();

        let δ13 = (δ12.sin()* a1.sin()* a2.sin()).atan2(a2.cos() + a1.cos()*cosα3);

        let φ3 = (φ1.sin()*δ13.cos() + φ1.cos()*δ13.sin()*θ13.cos()).max(-1.0).min(1.0).asin();

        let δλ13 = (θ13.sin()*δ13.sin()*φ1.cos()).atan2(δ13.cos() - φ1.sin()*φ3.sin());
        let λ3 = λ1 + δλ13;

        Some(Coords {
            lat: φ3.to_degrees(),
            lon: λ3.to_degrees()
        })
    }
}