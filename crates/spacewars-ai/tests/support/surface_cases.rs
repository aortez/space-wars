use scenario_spacewars::surface_sortie::pilot::MaterialFlightStart;

pub const STARTS: [MaterialFlightStart; 4] = [
    MaterialFlightStart {
        bearing: 0.0,
        altitude: 45.0,
        radial_speed: -4.0,
        lateral_speed: 3.0,
        heading_offset: 0.4,
    },
    MaterialFlightStart {
        bearing: std::f32::consts::PI,
        altitude: 60.0,
        radial_speed: -8.0,
        lateral_speed: -5.0,
        heading_offset: -0.7,
    },
    MaterialFlightStart {
        bearing: -std::f32::consts::FRAC_PI_2,
        altitude: 35.0,
        radial_speed: 2.0,
        lateral_speed: 6.0,
        heading_offset: 0.6,
    },
    MaterialFlightStart {
        bearing: 0.8,
        altitude: 75.0,
        radial_speed: -6.0,
        lateral_speed: -7.0,
        heading_offset: -0.5,
    },
];
