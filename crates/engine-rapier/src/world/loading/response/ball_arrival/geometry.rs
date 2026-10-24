//! Relative straight-line motion for the admitted radius-0.5 ball scene.
use super::*;

pub(super) fn delta(a: Vector, b: Vector) -> [f64; 2] {
    [
        f64::from(a.x) - f64::from(b.x),
        f64::from(a.y) - f64::from(b.y),
    ]
}

pub(super) fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(super) fn length(a: [f64; 2]) -> f64 {
    libm::hypot(a[0], a[1])
}

pub(super) fn check(p: [Vector; 3]) -> Result<(), BallArrivalRejection> {
    if p.iter().any(|p| !p.is_finite()) {
        return Err(BallArrivalRejection::Numerical);
    }
    for i in 0..3 {
        for j in i + 1..3 {
            if length(delta(p[j], p[i])) < 1.0 - 1.0e-5 {
                return Err(BallArrivalRejection::Numerical);
            }
        }
    }
    Ok(())
}

pub(super) fn next_time(
    p: [Vector; 3],
    speed: [Vector; 3],
) -> Result<Option<f64>, BallArrivalRejection> {
    check(p)?;
    if speed.iter().any(|v| !v.is_finite()) {
        return Err(BallArrivalRejection::Numerical);
    }
    let mut next: Option<f64> = None;
    for i in 0..3 {
        for j in i + 1..3 {
            let d = delta(p[j], p[i]);
            let v = delta(speed[i], speed[j]);
            let b = dot(d, v);
            if b <= 0.0 {
                continue;
            }
            let distance = length(d);
            let gap = distance - 1.0;
            if gap <= TOUCH {
                return Err(BallArrivalRejection::Unaccounted);
            }
            let a = dot(v, v);
            let cross = d[0] * v[1] - d[1] * v[0];
            // Cross product avoids subtracting two large squared distances.
            let closest = cross.abs() / libm::sqrt(a);
            if (closest - 1.0).abs() <= 1.0e-5 {
                return Err(BallArrivalRejection::Numerical);
            }
            if closest > 1.0 {
                continue;
            }
            let discriminant = a - cross * cross;
            let time = gap * (distance + 1.0) / (b + libm::sqrt(discriminant));
            if !time.is_finite() || time <= 0.0 {
                return Err(BallArrivalRejection::Numerical);
            }
            next = Some(next.map_or(time, |old| old.min(time)));
        }
    }
    Ok(next)
}
