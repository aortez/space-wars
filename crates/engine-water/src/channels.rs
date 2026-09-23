//! Per-outlet bounds for worlds containing both free ledges and narrow drains.
use crate::{SpillSource, WaterError, WaterWorld, finite_coordinate};

impl WaterWorld {
    /// Override the default channel for future parcels from one outlet (0 left,
    /// 1 right). `None` means free flight. Existing parcels keep their own bounds
    /// and detach from the source, avoiding a ribbon between unlike channels.
    /// Invalid input leaves geometry, liquid and presentation history unchanged.
    pub fn set_outlet_channel(
        &mut self,
        pool: usize,
        edge: usize,
        bounds: Option<[f64; 2]>,
    ) -> Result<(), WaterError> {
        let p = self.pools.get(pool).ok_or(WaterError::InvalidInput)?;
        if edge > 1
            || bounds.is_some_and(|[left, right]| {
                !finite_coordinate(left) || !finite_coordinate(right) || right - left < 0.001
            })
        {
            return Err(WaterError::InvalidInput);
        }
        // Changing a channel must not teleport newly emitted water sideways.
        let x = p.spec.left + edge as f64 * p.spec.column_width * p.spec.bed.len() as f64;
        if bounds.is_some_and(|[left, right]| x < left - 1e-8 || x > right + 1e-8) {
            return Err(WaterError::InvalidInput);
        }
        if p.outlet_channels[edge] == bounds {
            return Ok(());
        }
        self.pools[pool].outlet_channels[edge] = bounds;
        for spill in &mut self.spills {
            if spill
                .as_ref()
                .is_some_and(|s| s.source == (SpillSource::Outlet { pool, edge }))
            {
                *spill = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        tests::{DT, assert_accounting, spec},
        *,
    };

    #[test]
    fn ledges_keep_their_own_channels_while_the_floor_uses_a_narrow_drain() {
        let mut water = WaterWorld::new(
            WaterConfig {
                spill_channel: Some([-100.0, 100.0]),
                ..WaterConfig::default()
            },
            vec![
                spec(
                    -80.0,
                    20.0,
                    vec![60.0; 2],
                    [Boundary::Spill { lip: 60.0 }; 2],
                ),
                spec(
                    -100.0,
                    90.0,
                    vec![0.0; 8],
                    [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
                ),
            ],
        )
        .unwrap();
        water.set_outlet_channel(0, 0, None).unwrap();
        water.set_outlet_channel(0, 1, None).unwrap();
        water.set_outlet_channel(1, 1, Some([-10.0, 10.0])).unwrap();
        water.add_to_pool(0, -75.0, 20.0).unwrap();
        water.add_to_pool(0, -65.0, 20.0).unwrap();
        water.add_to_pool(1, -11.0, 20.0).unwrap();
        water.step(DT).unwrap();
        let mut seen = [false; 3];
        for (i, p) in water.parcels().iter().enumerate() {
            match water.spill_source(i).unwrap() {
                SpillSource::Outlet { pool: 0, edge } => {
                    assert_eq!(p.horizontal_bounds, None);
                    assert!(
                        p.position.x < -59.0,
                        "ledge must not jump to the drain: {p:?}"
                    );
                    seen[edge] = true;
                }
                SpillSource::Outlet { pool: 1, edge: 1 } => {
                    assert_eq!(p.horizontal_bounds, Some([-10.0, 10.0]));
                    assert!((-10.0..=10.0).contains(&p.position.x));
                    seen[2] = true;
                }
                other => panic!("unexpected source {other:?}"),
            }
        }
        assert!(seen.into_iter().all(|v| v));
        for _ in 0..240 {
            water.step(DT).unwrap();
            assert_accounting(&water);
        }
    }

    #[test]
    fn channel_changes_are_atomic_idempotent_and_do_not_rewrite_existing_parcels() {
        let mut water = WaterWorld::new(
            WaterConfig::default(),
            vec![spec(
                0.0,
                20.0,
                vec![0.0; 2],
                [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
            )],
        )
        .unwrap();
        water.add_to_pool(0, 19.0, 100.0).unwrap();
        water.step(DT).unwrap();
        let pools = water.pools().to_vec();
        let parcels = water.parcels().to_vec();
        let stats = water.stats();
        for bounds in [Some([0.0, 0.0]), Some([0.0, f64::NAN]), Some([-10.0, 10.0])] {
            assert_eq!(
                water.set_outlet_channel(0, 1, bounds),
                Err(WaterError::InvalidInput)
            );
        }
        assert_eq!(
            water.set_outlet_channel(1, 0, None),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(
            water.set_outlet_channel(0, 2, None),
            Err(WaterError::InvalidInput)
        );
        assert_eq!(water.pools(), pools);
        water.set_outlet_channel(0, 1, None).unwrap();
        assert!(water.spill_source(0).is_some()); // unchanged channel keeps history
        water.set_outlet_channel(0, 1, Some([20.0, 22.0])).unwrap();
        assert_eq!(water.parcels(), parcels);
        assert_eq!(water.stats(), stats);
        assert_eq!(water.spill_source(0), None);
        water.step(DT).unwrap();
        assert_eq!(water.parcels()[0].horizontal_bounds, None);
        assert_eq!(
            water.parcels().last().unwrap().horizontal_bounds,
            Some([20.0, 22.0])
        );
        assert_accounting(&water);
    }
}
