//! Experimental frictionless shared normal-motion budgets. This computes a compression
//! projection, not a second rigid-body simulation or a general contact law.
//! Inputs must include the whole participating set of touching, nonseparating
//! patches, with equivalent collider contacts already aggregated by the host.
//! The returned budget describes an ideal rigid compression response. A backend's
//! restitution setting alone need not realize it; see the `shared_response`
//! integration tests and `docs/design/material-shared-energy.md` before using it
//! to charge persistent regions. No production adapter consumes this API yet.

use engine_core::Vec2;

pub const MAX_BODIES: usize = 64;
pub const MAX_CONTACTS: usize = 32;
pub const MAX_ITERATIONS: usize = 2048;
/// Elastic pulse allocation is an explicit approximation, separate from the
/// isolated material law and the initial-work comparison model.
pub const PULSE_MODEL_VERSION: &str = "shared-elastic-pulse-v1";
const RESIDUAL_TOLERANCE: f64 = 1.0e-9;

#[derive(Debug, Clone, Copy)]
pub struct BodyMotion {
    pub center: Vec2,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
    /// Effective inverse properties, including locked axes. Prescribed axes
    /// have zero inverse properties and contribute explicitly attributed work.
    pub inverse_mass: Vec2,
    pub inverse_inertia: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Contact {
    /// Stable physical patch identity, never a backend contact index.
    pub id: u64,
    pub bodies: [usize; 2],
    pub point: Vec2,
    /// Unit normal from body 0 to body 1.
    pub normal: Vec2,
    /// Elastic series stiffness of the physical patch. This selects a
    /// stiffness-weighted split for redundant rows, not a new yield strength.
    pub stiffness: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactBudget {
    pub id: u64,
    pub component: usize,
    pub closing_speed: f64,
    /// Scalar loading speed carrying this share's energy and compression
    /// impulse. For pulse allocation this need not equal initial approach.
    pub loading_speed: f64,
    /// Nonnegative impulse for the coupled compression projection.
    pub compression_impulse: f64,
    /// Nonnegative share of the component budget, in energy units. `allocate`
    /// uses initial work; `allocate_pulse` uses elastic compliance weights.
    /// Neither split predicts that patch's full deformation history.
    pub energy: f64,
}

impl ContactBudget {
    /// Equivalent scalar input carrying this allocated energy into the material
    /// law. Its mass is a budget conversion, not the isolated contact mass.
    /// Conversion does not establish that the allocation represents the patch's
    /// actual loading history or that a backend can realize the ledger response.
    pub fn material_input(&self) -> Result<Option<crate::Impact>, BudgetError> {
        if self.energy == 0.0 {
            return Ok(None);
        }
        let input = crate::Impact {
            closing_speed: self.loading_speed as f32,
            effective_mass: (2.0 * self.energy / self.loading_speed.powi(2)) as f32,
        };
        input.validate().map_err(|_| BudgetError::Range)?;
        let converted = f64::from(input.energy());
        if !converted.is_finite() || (converted - self.energy).abs() > 1.0e-6 * self.energy {
            return Err(BudgetError::Range);
        }
        Ok(Some(input))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentBudget {
    pub available: f64,
    /// Signed dynamic kinetic energy removed by the compression impulse.
    pub kinetic_removed: f64,
    /// Signed work supplied by prescribed axes during compression.
    /// `available = kinetic_removed + prescribed_work`.
    pub prescribed_work: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// Ordered by stable contact identity.
    pub contacts: Vec<ContactBudget>,
    /// Coupled normal modes. Each needs its own common restitution; unrelated
    /// components must not exchange their material's recoverable energy.
    pub components: Vec<ComponentBudget>,
    pub iterations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetError {
    Limit,
    InvalidInput,
    DuplicateContact,
    SeparatingContact,
    ImmovableContact,
    /// Ill-conditioned or incompatible driven contacts did not reach the
    /// complementarity tolerance within the fixed iteration bound.
    NotConverged,
    Range,
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shared impact: {self:?}")
    }
}
impl std::error::Error for BudgetError {}

struct Row {
    contact: Contact,
    jacobian: [[f64; 3]; 2],
    speed: f64,
    prescribed_speed: f64,
}

fn properties(body: &BodyMotion) -> [f64; 3] {
    [
        f64::from(body.inverse_mass.x),
        f64::from(body.inverse_mass.y),
        f64::from(body.inverse_inertia),
    ]
}

fn velocity(body: &BodyMotion) -> [f64; 3] {
    [
        f64::from(body.linear_velocity.x),
        f64::from(body.linear_velocity.y),
        f64::from(body.angular_velocity),
    ]
}

fn valid_body(b: &BodyMotion) -> bool {
    [
        b.center.x,
        b.center.y,
        b.linear_velocity.x,
        b.linear_velocity.y,
        b.angular_velocity,
        b.inverse_mass.x,
        b.inverse_mass.y,
        b.inverse_inertia,
    ]
    .into_iter()
    .all(f32::is_finite)
        && properties(b).into_iter().all(|x| x >= 0.0)
}

/// Solve `lambda >= 0`, `A lambda - u >= 0`,
/// `lambda_i * (A lambda - u)_i = 0`, where `A = J M^-1 J^T`.
///
/// The energy is `lambda·u - 0.5 lambda·A lambda`; isolated contact reduces to
/// `0.5 m_eff u²`. Redundant rows share motion instead of each receiving that
/// whole isolated budget. No timestep, gravity, friction or joint response is
/// inferred. The caller must reject incomplete contact sets and account for
/// constraints/external forces separately. Failure returns no partial budget.
pub fn allocate(bodies: &[BodyMotion], contacts: &[Contact]) -> Result<Budget, BudgetError> {
    allocate_model(bodies, contacts, false)
}

/// Allocate the coupled budget by `compression_impulse² / stiffness`.
/// This assumes a common pulse duration/shape and elastic series compliance,
/// so a resting contact carrying impulse receives work. It does not simulate
/// wave propagation, nonlinear redistribution or repeated compression cycles.
/// Equivalent scalar inputs preserve each share's energy and compression
/// impulse; their loading speed is not the measured initial approach speed.
pub fn allocate_pulse(bodies: &[BodyMotion], contacts: &[Contact]) -> Result<Budget, BudgetError> {
    allocate_model(bodies, contacts, true)
}

fn allocate_model(
    bodies: &[BodyMotion],
    contacts: &[Contact],
    pulse: bool,
) -> Result<Budget, BudgetError> {
    if bodies.len() > MAX_BODIES || contacts.len() > MAX_CONTACTS {
        return Err(BudgetError::Limit);
    }
    if !bodies.iter().all(valid_body) {
        return Err(BudgetError::InvalidInput);
    }
    let mut ordered = contacts.to_vec();
    ordered.sort_by_key(|c| c.id);
    if ordered.windows(2).any(|c| c[0].id == c[1].id) {
        return Err(BudgetError::DuplicateContact);
    }
    let mut rows = Vec::new();
    for c in ordered {
        if c.bodies[0] == c.bodies[1]
            || c.bodies.iter().any(|i| *i >= bodies.len())
            || !c.stiffness.is_finite()
            || c.stiffness <= 0.0
            || ![c.point.x, c.point.y, c.normal.x, c.normal.y]
                .into_iter()
                .all(f32::is_finite)
            || (f64::from(c.normal.x).powi(2) + f64::from(c.normal.y).powi(2) - 1.0).abs() > 1.0e-5
        {
            return Err(BudgetError::InvalidInput);
        }
        let mut row = Row {
            contact: c,
            jacobian: [[0.0; 3]; 2],
            speed: 0.0,
            prescribed_speed: 0.0,
        };
        for side in 0..2 {
            let body = &bodies[c.bodies[side]];
            let sign = if side == 0 { 1.0 } else { -1.0 };
            let nx = f64::from(c.normal.x) * sign;
            let ny = f64::from(c.normal.y) * sign;
            row.jacobian[side] = [
                nx,
                ny,
                (f64::from(c.point.x) - f64::from(body.center.x)) * ny
                    - (f64::from(c.point.y) - f64::from(body.center.y)) * nx,
            ];
            for axis in 0..3 {
                let speed = row.jacobian[side][axis] * velocity(body)[axis];
                row.speed += speed;
                if properties(body)[axis] == 0.0 {
                    row.prescribed_speed += speed;
                }
            }
        }
        if row.speed < 0.0 {
            return Err(BudgetError::SeparatingContact);
        }
        rows.push(row);
    }
    let n = rows.len();
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut a = 0.0;
            for s in 0..2 {
                for t in 0..2 {
                    if rows[i].contact.bodies[s] == rows[j].contact.bodies[t] {
                        let inverse = properties(&bodies[rows[i].contact.bodies[s]]);
                        for (axis, inv) in inverse.into_iter().enumerate() {
                            a += rows[i].jacobian[s][axis] * inv * rows[j].jacobian[t][axis];
                        }
                    }
                }
            }
            if !a.is_finite() {
                return Err(BudgetError::Range);
            }
            matrix[i][j] = a;
            matrix[j][i] = a;
        }
        if matrix[i][i] <= 0.0 {
            return Err(BudgetError::ImmovableContact);
        }
    }
    let mut result = Budget {
        contacts: Vec::new(),
        components: Vec::new(),
        iterations: 0,
    };
    let mut visited = vec![false; n];
    for first in 0..n {
        if visited[first] {
            continue;
        }
        let mut group = vec![first];
        visited[first] = true;
        let mut cursor = 0;
        while cursor < group.len() {
            let i = group[cursor];
            for j in 0..n {
                if !visited[j] && matrix[i][j] != 0.0 {
                    visited[j] = true;
                    group.push(j);
                }
            }
            cursor += 1;
        }
        group.sort_unstable();
        let (impulses, iterations) = solve(&rows, &matrix, &group)?;
        result.iterations += iterations;
        let mut dot = 0.0;
        let mut quadratic = 0.0;
        let mut driver = 0.0;
        for (a, &i) in group.iter().enumerate() {
            dot += impulses[a] * rows[i].speed;
            driver += impulses[a] * rows[i].prescribed_speed;
            for (b, &j) in group.iter().enumerate() {
                quadratic += impulses[a] * matrix[i][j] * impulses[b];
            }
        }
        let available = dot - 0.5 * quadratic;
        if !available.is_finite() || !driver.is_finite() || available < 0.0 {
            return Err(BudgetError::Range);
        }
        let max_impulse = impulses.iter().copied().fold(0.0, f64::max);
        let min_stiffness = group
            .iter()
            .map(|&i| f64::from(rows[i].contact.stiffness))
            .fold(f64::INFINITY, f64::min);
        let weights: Vec<_> = group
            .iter()
            .enumerate()
            .map(|(a, &i)| {
                if pulse && max_impulse > 0.0 {
                    (impulses[a] / max_impulse).powi(2)
                        * (min_stiffness / f64::from(rows[i].contact.stiffness))
                } else {
                    impulses[a] * rows[i].speed
                }
            })
            .collect();
        let weight_sum: f64 = weights.iter().sum();
        if !weight_sum.is_finite() || (available > 0.0 && weight_sum == 0.0) {
            return Err(BudgetError::Range);
        }
        for (a, &i) in group.iter().enumerate() {
            // Normalize the nonnegative complementarity shares to the actual
            // extracted energy instead of silently dropping the solver residual.
            let energy = if weight_sum > 0.0 {
                available * (weights[a] / weight_sum)
            } else {
                0.0
            };
            result.contacts.push(ContactBudget {
                id: rows[i].contact.id,
                component: result.components.len(),
                closing_speed: rows[i].speed,
                loading_speed: if pulse && impulses[a] > 0.0 {
                    2.0 * energy / impulses[a]
                } else {
                    rows[i].speed
                },
                compression_impulse: impulses[a],
                energy,
            });
        }
        result.components.push(ComponentBudget {
            available,
            kinetic_removed: available - driver,
            prescribed_work: driver,
        });
    }
    result.contacts.sort_by_key(|c| c.id);
    Ok(result)
}

fn solve(
    rows: &[Row],
    matrix: &[Vec<f64>],
    group: &[usize],
) -> Result<(Vec<f64>, usize), BudgetError> {
    let n = group.len();
    let max_stiffness = group
        .iter()
        .map(|i| f64::from(rows[*i].contact.stiffness))
        .fold(0.0, f64::max);
    let scale: Vec<_> = group
        .iter()
        .map(|i| libm::sqrt(f64::from(rows[*i].contact.stiffness) / max_stiffness))
        .collect();
    let mut b = vec![vec![0.0; n]; n];
    let mut lipschitz = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            b[i][j] = scale[i] * matrix[group[i]][group[j]] * scale[j];
        }
        lipschitz = lipschitz.max(b[i].iter().map(|x| x.abs()).sum());
    }
    let speed_scale = group.iter().map(|i| rows[*i].speed).fold(0.0, f64::max);
    if speed_scale == 0.0 {
        return Ok((vec![0.0; n], 0));
    }
    let mut x = vec![0.0; n];
    let mut extrapolated = x.clone();
    let mut acceleration = 1.0_f64;
    for iteration in 1..=MAX_ITERATIONS {
        let next: Vec<_> = (0..n)
            .map(|i| {
                let gradient = b[i]
                    .iter()
                    .zip(&extrapolated)
                    .map(|(a, v)| a * v)
                    .sum::<f64>()
                    - scale[i] * rows[group[i]].speed;
                (extrapolated[i] - gradient / lipschitz).max(0.0)
            })
            .collect();
        let impulses: Vec<_> = next.iter().zip(&scale).map(|(x, s)| x * s).collect();
        if !impulses.iter().all(|p| p.is_finite()) {
            return Err(BudgetError::Range);
        }
        let max_impulse = impulses.iter().copied().fold(0.0, f64::max);
        let converged = group.iter().enumerate().all(|(i, &row)| {
            let residual = rows[row].speed
                - group
                    .iter()
                    .enumerate()
                    .map(|(j, &col)| matrix[row][col] * impulses[j])
                    .sum::<f64>();
            let violation = if impulses[i] > max_impulse * 1.0e-12 {
                residual.abs()
            } else {
                residual.max(0.0)
            };
            violation <= RESIDUAL_TOLERANCE * speed_scale
        });
        if converged {
            return Ok((impulses, iteration));
        }
        let next_acceleration = 0.5 * (1.0 + libm::sqrt(1.0 + 4.0 * acceleration * acceleration));
        for i in 0..n {
            extrapolated[i] = next[i] + (acceleration - 1.0) / next_acceleration * (next[i] - x[i]);
        }
        x = next;
        acceleration = next_acceleration;
    }
    Err(BudgetError::NotConverged)
}

#[cfg(test)]
mod tests;
