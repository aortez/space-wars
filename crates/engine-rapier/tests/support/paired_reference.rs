//! Continuous wood/armor series response, independent of the discrete solver.
const KW: f64 = 400_000.0;
const KA: f64 = 4_000_000.0;
const Y: f64 = 6000.0;
const ETA: f64 = 1500.0;
const K: f64 = KW * KA / (KW + KA);

pub struct Reference {
    mass: f64,
    pub x: f64,
    pub v: f64,
    pub p: f64,
    pub loss: f64,
    force: f64,
    phase: u8,
}
impl Reference {
    pub fn new(mass: f64, speed: f64) -> Self {
        Self {
            mass,
            x: 0.0,
            v: speed,
            p: 0.0,
            loss: 0.0,
            force: 0.0,
            phase: 0,
        }
    }

    pub fn advance(&mut self, dt: f64) {
        // The interface has no mass. Wood flows at (F-Y)/eta while armor
        // stores F/KA displacement: F' = KA*(v-(F-Y)/eta), v' = -F/mu.
        let mut left = dt;
        while left > 0.0 {
            let h = left.min(1.0 / 4_194_304.0);
            if self.phase == 3 {
                self.x += self.v * h;
            } else if self.phase == 1 {
                let state = [self.force, self.v, self.p, self.loss];
                let derivative = |s: [f64; 4]| {
                    let flow = (s[0] - Y) / ETA;
                    [
                        KA * (s[1] - flow),
                        -s[0] / self.mass,
                        flow,
                        ETA * flow * flow,
                    ]
                };
                let advance =
                    |s: [f64; 4], d: [f64; 4], h| std::array::from_fn(|i| s[i] + h * d[i]);
                let a = derivative(state);
                let b = derivative(advance(state, a, h / 2.0));
                let c = derivative(advance(state, b, h / 2.0));
                let d = derivative(advance(state, c, h));
                let next: [f64; 4] = std::array::from_fn(|i| {
                    state[i] + h / 6.0 * (a[i] + 2.0 * b[i] + 2.0 * c[i] + d[i])
                });
                [self.force, self.v, self.p, self.loss] = next;
                self.x = self.p + Y / KW + self.force / KA;
                if self.force < Y {
                    self.phase = 2;
                }
            } else {
                let omega = (K / self.mass).sqrt();
                let (sin, cos) = (omega * h).sin_cos();
                let e = self.x - self.p;
                self.x = self.p + e * cos + self.v / omega * sin;
                self.v = self.v * cos - e * omega * sin;
                self.force = K * (self.x - self.p);
                if self.phase == 0 && self.force > Y {
                    self.phase = 1;
                }
                if self.x < self.p {
                    self.phase = 3;
                }
            }
            left -= h;
        }
    }
}
