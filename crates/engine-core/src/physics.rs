//! Pure physics helpers shared by Spacewars entities.

use crate::constants::GRAVITY;

pub fn gravity_force(mass_a: f32, mass_b: f32, distance: f32) -> f32 {
    if distance == 0.0 {
        0.0
    } else {
        GRAVITY * mass_a * mass_b / (distance * distance)
    }
}

pub fn gravity_acceleration_attracted_to(mass_attractor: f32, distance: f32, scale: f32) -> f32 {
    if distance == 0.0 {
        0.0
    } else {
        GRAVITY * mass_attractor / (distance * distance) * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-6;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn gravity_force_matches_model_formula() {
        assert_close(gravity_force(10.0, 20.0, 5.0), 0.04);
    }

    #[test]
    fn gravity_acceleration_is_force_divided_by_body_mass() {
        let mass_body = 10.0;
        let mass_planet = 20.0;
        let distance = 5.0;
        let scale = 7.0;

        assert_close(
            gravity_acceleration_attracted_to(mass_planet, distance, scale),
            gravity_force(mass_body, mass_planet, distance) * scale / mass_body,
        );
    }

    #[test]
    fn zero_distance_does_not_generate_infinity() {
        assert_eq!(gravity_force(1.0, 1.0, 0.0), 0.0);
        assert_eq!(gravity_acceleration_attracted_to(1.0, 0.0, 1.0), 0.0);
    }
}
