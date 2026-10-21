//! Contact and beam geometry, and the kinematic helpers that turn two bodies'
//! motion into the scalar inputs of the material law.

use crate::MaterialError;
use engine_core::Vec2;
use serde::Serialize;

/// Bounded patch-width approximation: this fraction of the effective radius
/// of the two curved surfaces. It is a documented approximation, not a
/// load-dependent Hertz contact.
pub const PATCH_WIDTH_FRACTION: f32 = 0.5;

/// Default deformation depth as a fraction of the patch width. Stress spreads
/// below a contact about as deep as the patch is wide; elastic line-contact
/// solutions put the effective depth at one to a few widths, growing slowly
/// with body size, so one width is the stiff end of that range.
pub const DEPTH_WIDTH_FRACTION: f32 = 1.0;

fn length_ok(value: f32) -> bool {
    value.is_finite() && (1.0e-6..=1.0e6).contains(&value)
}

/// Material geometry behind one side of a contact.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Section {
    /// Represented depth into the screen, `wu`.
    pub thickness: f32,
    /// Effective depth of material that deforms along the contact normal, `wu`.
    /// [`deformation_depth`] gives the default.
    pub depth: f32,
}

impl Section {
    pub const fn new(thickness: f32, depth: f32) -> Self {
        Self { thickness, depth }
    }

    pub fn validate(self) -> Result<(), MaterialError> {
        if length_ok(self.thickness) && length_ok(self.depth) {
            Ok(())
        } else {
            Err(MaterialError::InvalidGeometry)
        }
    }

    /// Linear stiffness of this material behind a patch of `area`: force per
    /// unit normal deformation, `stiffness * area / depth`.
    pub fn contact_stiffness(self, stiffness: f32, area: f32) -> f32 {
        stiffness * area / self.depth
    }
}

/// Rectangular beam section for structural connections, `wu`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Beam {
    /// Represented depth into the screen.
    pub thickness: f32,
    /// In-plane section height, the bending direction.
    pub height: f32,
    /// Connection length over which bending and extension develop.
    pub length: f32,
}

impl Beam {
    pub const fn new(thickness: f32, height: f32, length: f32) -> Self {
        Self {
            thickness,
            height,
            length,
        }
    }

    pub fn validate(self) -> Result<(), MaterialError> {
        if length_ok(self.thickness) && length_ok(self.height) && length_ok(self.length) {
            Ok(())
        } else {
            Err(MaterialError::InvalidGeometry)
        }
    }

    pub fn area(self) -> f32 {
        self.thickness * self.height
    }

    /// `thickness * height^2 / 6`, the same form as the terrain cantilever check.
    pub fn section_modulus(self) -> f32 {
        self.thickness * self.height * self.height / 6.0
    }

    /// `thickness * height^3 / 12`.
    pub fn second_moment(self) -> f32 {
        self.thickness * self.height * self.height * self.height / 12.0
    }

    pub fn volume(self) -> f32 {
        self.thickness * self.height * self.length
    }
}

/// The in-plane contact patch shared by two regions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ContactGeometry {
    /// In-plane patch width, `wu`.
    pub width: f32,
    pub a: Section,
    pub b: Section,
}

impl ContactGeometry {
    pub const fn new(width: f32, a: Section, b: Section) -> Self {
        Self { width, a, b }
    }

    pub fn validate(self) -> Result<(), MaterialError> {
        if !length_ok(self.width) {
            return Err(MaterialError::InvalidGeometry);
        }
        self.a.validate()?;
        self.b.validate()
    }

    /// Load-bearing area: the patch width times the thinner represented
    /// thickness, since only overlapping material carries the contact.
    pub fn area(self) -> f32 {
        self.width * self.a.thickness.min(self.b.thickness)
    }

    /// Combine equivalent sub-contacts of one physical episode. Widths add and
    /// the sections must agree, so splitting a collider cannot multiply energy.
    /// Derive the sections' depths from the merged width, not each part's.
    pub fn merge(parts: &[ContactGeometry]) -> Result<Self, MaterialError> {
        let first = *parts.first().ok_or(MaterialError::InvalidGeometry)?;
        let mut width = 0.0;
        for part in parts {
            part.validate()?;
            if part.a != first.a || part.b != first.b {
                return Err(MaterialError::InvalidGeometry);
            }
            width += part.width;
        }
        Ok(Self { width, ..first })
    }
}

/// Patch width for a curved surface of `radius_a` meeting another curved
/// surface, or a flat one when `radius_b` is `None`.
pub fn patch_width_from_radii(radius_a: f32, radius_b: Option<f32>) -> Result<f32, MaterialError> {
    if !length_ok(radius_a) {
        return Err(MaterialError::InvalidGeometry);
    }
    let effective = match radius_b {
        Some(radius_b) => {
            if !length_ok(radius_b) {
                return Err(MaterialError::InvalidGeometry);
            }
            radius_a * radius_b / (radius_a + radius_b)
        }
        None => radius_a,
    };
    Ok(PATCH_WIDTH_FRACTION * effective)
}

/// Default deformation depth behind a contact patch of `width`: the width
/// times `DEPTH_WIDTH_FRACTION`, capped by `extent`, how far the material
/// reaches behind the contact along the normal, such as a plate's thickness.
/// `None` is a half-space.
pub fn deformation_depth(width: f32, extent: Option<f32>) -> Result<f32, MaterialError> {
    if !length_ok(width) || extent.is_some_and(|extent| !length_ok(extent)) {
        return Err(MaterialError::InvalidGeometry);
    }
    let depth = DEPTH_WIDTH_FRACTION * width;
    Ok(extent.map_or(depth, |extent| depth.min(extent)))
}

/// One body's share of the contact inertia.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ContactBody {
    /// `None` is immovable: a static or kinematic body.
    pub mass: Option<f32>,
    /// `None` cannot rotate about the contact: locked rotation or immovable.
    pub inertia: Option<f32>,
    /// Signed lever arm `offset x normal` from the center of mass to the
    /// contact point, `wu`. See [`lever`].
    pub lever: f32,
}

impl ContactBody {
    pub const IMMOVABLE: Self = Self {
        mass: None,
        inertia: None,
        lever: 0.0,
    };

    pub const fn translating(mass: f32) -> Self {
        Self {
            mass: Some(mass),
            inertia: None,
            lever: 0.0,
        }
    }

    pub const fn rotating(mass: f32, inertia: f32, lever: f32) -> Self {
        Self {
            mass: Some(mass),
            inertia: Some(inertia),
            lever,
        }
    }

    fn inverse_mass(self) -> Result<f32, MaterialError> {
        let mut inverse = 0.0;
        if let Some(mass) = self.mass {
            if !(mass.is_finite() && mass > 0.0) {
                return Err(MaterialError::InvalidLoad);
            }
            inverse += 1.0 / mass;
        }
        if let Some(inertia) = self.inertia {
            if !(inertia.is_finite() && inertia > 0.0 && self.lever.is_finite()) {
                return Err(MaterialError::InvalidLoad);
            }
            inverse += self.lever * self.lever / inertia;
        }
        Ok(inverse)
    }
}

/// Contact-effective mass of two bodies along the normal:
/// `1 / m_eff = sum(1 / m_i + lever_i^2 / I_i)` over the movable bodies.
pub fn effective_mass(a: ContactBody, b: ContactBody) -> Result<f32, MaterialError> {
    let inverse = a.inverse_mass()? + b.inverse_mass()?;
    if inverse > 0.0 && inverse.is_finite() {
        Ok(1.0 / inverse)
    } else {
        Err(MaterialError::InvalidLoad)
    }
}

/// Signed lever arm of a contact: the 2D cross product `offset x normal`.
pub fn lever(offset: Vec2, normal: Vec2) -> f32 {
    offset.x * normal.y - offset.y * normal.x
}

/// Velocity of a body point at `offset` from the center of mass.
pub fn point_velocity(linear: Vec2, angular: f32, offset: Vec2) -> Vec2 {
    linear + Vec2::new(-offset.y, offset.x) * angular
}

/// Normal approach speed of `a` toward `b`, where `normal` points from `a`
/// to `b`. Separating motion is zero, and a common velocity cancels.
pub fn closing_speed(normal: Vec2, velocity_a: Vec2, velocity_b: Vec2) -> f32 {
    (velocity_a - velocity_b).dot(normal).max(0.0)
}
