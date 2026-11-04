//! Continuous falling-platen reference, independent of the native discrete solve.
pub const K: f64 = 400_000.0;
pub const Y: f64 = 6000.0;
pub const ETA: f64 = 1500.0;
pub const EY: f64 = Y / K;
#[derive(Debug)]
pub struct Reference {
    pub x: f64,
    pub v: f64,
    pub p: f64,
    pub loss: f64,
}
impl Reference {
    pub fn elastic(&self) -> f64 {
        0.5 * K * (self.x - self.p).max(0.0).powi(2)
    }
}
pub fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}; tolerance {tolerance}"
    );
}
pub fn arrival(m: f64, weight: f64, x: f64, v: f64) -> (f64, f64) {
    let speed = libm::sqrt(v * v - 2.0 * weight / m * x);
    let time = if x == 0.0 {
        0.0
    } else {
        -2.0 * x / (v + speed)
    };
    (time, speed)
}
// Released elastic energy after a plastic turn. Below half yield weight, the
// body separates and returns to the displaced surface. Otherwise it oscillates
// in sustained contact; no artificial damping forces it to rest.
fn released(m: f64, weight: f64, t: f64) -> (f64, f64) {
    let omega = libm::sqrt(K / m);
    let eq = weight / K;
    let amplitude = EY - eq;
    if EY <= 2.0 * eq {
        return (
            eq + amplitude * libm::cos(omega * t),
            -amplitude * omega * libm::sin(omega * t),
        );
    }
    let exit = libm::acos(-eq / amplitude) / omega;
    let speed = libm::sqrt(K / m * (EY * EY - 2.0 * eq * EY));
    let flight = if weight == 0.0 {
        f64::INFINITY
    } else {
        2.0 * speed * m / weight
    };
    let period = 2.0 * exit + flight;
    let local = t % period;
    if local < exit {
        (
            eq + amplitude * libm::cos(omega * local),
            -amplitude * omega * libm::sin(omega * local),
        )
    } else if local < exit + flight {
        let u = local - exit;
        (
            -speed * u + 0.5 * weight / m * u * u,
            -speed + weight / m * u,
        )
    } else {
        let u = period - local;
        (
            eq + amplitude * libm::cos(omega * u),
            amplitude * omega * libm::sin(omega * u),
        )
    }
}
pub fn reference(m: f64, weight: f64, x0: f64, v0: f64, t: f64) -> Reference {
    let (at, incoming) = arrival(m, weight, x0, v0);
    let omega = libm::sqrt(K / m);
    let eq = weight / K;
    let amplitude = libm::sqrt(eq * eq + (incoming / omega).powi(2));
    let r = if t < at {
        Reference {
            x: x0 + v0 * t + 0.5 * weight / m * t * t,
            v: v0 + weight / m * t,
            p: 0.0,
            loss: 0.0,
        }
    } else if eq + amplitude <= EY {
        let exit = (2.0 * core::f64::consts::PI - 2.0 * libm::atan2(incoming / omega, eq)) / omega;
        let flight = if weight == 0.0 {
            f64::INFINITY
        } else {
            2.0 * incoming * m / weight
        };
        let local = (t - at) % (exit + flight);
        let (x, v) = if local <= exit {
            (
                eq * (1.0 - libm::cos(omega * local)) + incoming / omega * libm::sin(omega * local),
                eq * omega * libm::sin(omega * local) + incoming * libm::cos(omega * local),
            )
        } else {
            let u = local - exit;
            (
                -incoming * u + 0.5 * weight / m * u * u,
                -incoming + weight / m * u,
            )
        };
        Reference {
            x,
            v,
            p: 0.0,
            loss: 0.0,
        }
    } else {
        let yield_time =
            (libm::asin((EY - eq) / amplitude) + libm::atan2(eq, incoming / omega)) / omega;
        let elapsed = t - at;
        if elapsed <= yield_time {
            Reference {
                x: eq * (1.0 - libm::cos(omega * elapsed))
                    + incoming / omega * libm::sin(omega * elapsed),
                v: eq * omega * libm::sin(omega * elapsed) + incoming * libm::cos(omega * elapsed),
                p: 0.0,
                loss: 0.0,
            }
        } else {
            let vy = libm::sqrt(incoming * incoming + (2.0 * weight * EY - K * EY * EY) / m);
            let terminal = (weight - Y) / ETA;
            let rate = ETA / m;
            let turn = if weight < Y {
                libm::log1p(vy / -terminal) / rate
            } else {
                f64::INFINITY
            };
            let u = (elapsed - yield_time).min(turn);
            let extra = vy - terminal;
            let p = terminal * u + extra * -libm::expm1(-rate * u) / rate;
            let loss = ETA
                * (terminal * terminal * u
                    + 2.0 * terminal * extra * -libm::expm1(-rate * u) / rate
                    + extra * extra * -libm::expm1(-2.0 * rate * u) / (2.0 * rate));
            if elapsed - yield_time <= turn {
                Reference {
                    x: p + EY,
                    v: terminal + extra * libm::exp(-rate * u),
                    p,
                    loss,
                }
            } else {
                let (e, v) = released(m, weight, elapsed - yield_time - turn);
                Reference {
                    x: p + e,
                    v,
                    p,
                    loss,
                }
            }
        }
    };
    near(
        0.5 * m * v0 * v0 + weight * (r.x - x0),
        0.5 * m * r.v * r.v + r.elastic() + Y * r.p + r.loss,
        1e-8,
    );
    r
}
