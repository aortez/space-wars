use super::*;

impl Duel {
    pub(crate) fn insert(
        physics: &mut PhysicsWorld,
        mut config: ScorchedConfig,
        seed: u64,
        placement: host::Placement,
    ) -> Self {
        config.wheel_tuning = config.wheel_tuning.bounded();
        config.max_grains = config.max_grains.clamp(1, 512);
        config.bombardment_seconds = config.bombardment_seconds.min(600);
        if config.course.is_some() {
            config.roof = false;
            config.hillside = false;
            config.undercut = false;
            config.drop_height = None;
            config.bombardment_seconds = 0;
        }
        if config.roof {
            config.hillside = false;
        }
        if config.hillside || config.roof {
            config.undercut = false;
            config.drop_height = None;
            config.bombardment_seconds = 0;
        }
        if config.drop_height.is_some() {
            config.undercut = false;
        }
        if config.undercut || config.drop_height.is_some() {
            config.demo = false;
            config.bombardment_seconds = 0;
        }
        let field = if let Some(course) = config.course {
            course.terrain()
        } else if config.roof {
            roof::terrain()
        } else if config.hillside {
            hillside::terrain(seed)
        } else if config.drop_height.is_some() {
            drop_test::floor()
        } else if config.undercut {
            bank::terrain()
        } else {
            Terrain::generate(
                placement.terrain_width,
                FIELD_HEIGHT,
                CELL_SIZE * placement.scale,
                vec![Material {
                    id: DIRT,
                    hardness: 100,
                }],
                |c| {
                    let x = (c.x as f32 + 0.5 - placement.terrain_width as f32 * 0.5) * CELL_SIZE;
                    let y = (c.y as f32 + 0.5 - FIELD_HEIGHT as f32 * 0.5) * CELL_SIZE;
                    if y <= placement.hill(x, seed) {
                        DIRT
                    } else {
                        MaterialId::VOID
                    }
                },
            )
            .expect("bounded hill field")
        };
        let mut initial_cells = field
            .cells()
            .iter()
            .filter(|c| c.material != MaterialId::VOID)
            .count() as u64;
        let geometry = TerrainGeometry::with_surface(&field, TerrainSurface::Interpolated);
        let assembly = TerrainAssembly::insert(
            physics,
            placement.ground(),
            BodySpec {
                kind: BodyKind::Fixed,
                position: placement.origin,
                ..Default::default()
            },
            &field,
            &geometry,
            TerrainSpec {
                surface: TerrainSurface::Interpolated,
                ..Default::default()
            },
        )
        .expect("valid ground");
        let ground = TerrainFragment {
            id: placement.ground(),
            hash: field.hash(),
            terrain: field,
            geometry,
            assembly,
            edited_chunks: Vec::new(),
        };
        let mut running_gear = [None, None];
        let tanks = std::array::from_fn(|index| {
            let id = PhysicsId::new(placement.ids.start + 10 + index as u64);
            let mut body = BodyId::new(id, BodyRole::PRIMARY);
            let x = if config.course.is_some() {
                if index == 0 { course::START_X } else { 38.0 }
            } else if config.roof {
                if index == 0 { 0.0 } else { 4.0 }
            } else if index == 0 {
                if config.hillside {
                    -1.0
                } else {
                    placement.spawn_x[index]
                }
            } else {
                placement.spawn_x[index]
            };
            let mut collider = ColliderSpec::convex_polygon(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                RIGID_TANK_HULL.to_vec(),
            );
            if let engine_rapier::world::ColliderShape::ConvexPolygon { vertices } =
                &mut collider.shape
            {
                for vertex in vertices {
                    *vertex *= placement.scale;
                }
            }
            collider.density = 8.0;
            collider.friction = 1.0;
            let position = Vec2::new(
                x,
                if let Some(course) = config.course {
                    course.floor(x) + 2.0
                } else if config.roof {
                    if index == 0 { 15.0 } else { -8.7 }
                } else if config.drop_height.is_some() {
                    -8.0
                } else if config.undercut || config.hillside {
                    if index == 0 {
                        9.0
                    } else if config.hillside {
                        -8.7
                    } else {
                        -8.0
                    }
                } else {
                    placement.hill(x, seed) + 2.0
                },
            );
            if suspension::enabled(config) {
                let assembly = suspension::insert_scaled(
                    physics,
                    id,
                    placement.point_to_world(position),
                    config.wheel_tuning,
                    placement.scale,
                );
                body = assembly.bodies()[0];
                running_gear[index] = Some(assembly);
            } else {
                assert!(physics.insert_body(
                    body,
                    BodySpec {
                        position: placement.point_to_world(position),
                        angular_damping: 2.0,
                        ccd_enabled: true,
                        ..Default::default()
                    },
                    &[collider]
                ));
            }
            Tank {
                body,
                elevation: 50.0_f32.to_radians(),
                speed: 32.0,
                facing: if index == 0 { 1.0 } else { -1.0 },
                health: 100.0,
                next_shot: if config.course.is_some() {
                    u64::MAX
                } else if config.hillside || config.roof {
                    0
                } else {
                    index as u64 * 300
                },
                shots: 0,
            }
        });
        let mut terrain = vec![ground];
        if let Some(height) = config.drop_height {
            let clump = drop_test::clump(physics, height);
            initial_cells += clump
                .terrain
                .cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count() as u64;
            terrain.push(clump);
        }
        let driving = std::array::from_fn(|i| {
            driving::TankDrive::new(
                placement.point_from_world(physics.motion(tanks[i].body).unwrap().position),
            )
        });
        Self {
            tick: 0,
            config,
            impacts: 0,
            rejected_blasts: 0,
            last_capacity_rejection: None,
            shots: 0,
            last_physics: PhysicsStepMetrics::default(),
            seed,
            placement: placement.clone(),
            firing: true,
            retired: [None, None],
            impact_events: Vec::with_capacity(MAX_SHELLS),
            terrain,
            loose: LooseTerrain::new(LooseTerrainConfig {
                max_grains: config.max_grains - placement.reserved_grains,
                shape: config.shape,
                slumping: config.slumping.then(Default::default),
                cohesion: config.cohesion.config().map(|mut cohesion| {
                    cohesion.strength *= placement.scale.powi(2);
                    cohesion
                }),
                impact_fracture: config
                    .breakup
                    .then(|| config.cohesion.impact_config())
                    .flatten()
                    .map(|mut impact| {
                        impact.yield_speed *= placement.scale;
                        impact
                    }),
                ..Default::default()
            })
            .expect("bounded loose material"),
            next_id: placement.ids.start + 100,
            initial_cells,
            tanks,
            shells: Vec::new(),
            bots: Default::default(),
            shot_observations: Vec::new(),
            selected: if config.hillside || config.roof { 1 } else { 0 },
            previous: Controls::default(),
            driving,
            course_goal: config
                .course
                .map(|course| driving_lab::TravelGoal::new(course.finish_x(), 2.5)),
            course_decision: None,
            running_gear,
            manual_drive_armed: false,
            select_used: false,
            pointer_drive: 0.0,
            nudge_drive: 0.0,
            nudge_until: 0,
            flashes: Vec::new(),
            status: "Drive into position, aim and fire. Demo controls both tanks.",
        }
    }
    pub fn loose_cells(&self) -> usize {
        self.loose.len()
    }
    pub fn deposited_cells(&self) -> u64 {
        self.loose.deposited_cells()
    }
    pub fn settling_diagnostics(&self) -> engine_rapier::terrain::SettlingDiagnostics {
        self.loose.settling_diagnostics()
    }
    pub fn settling_work(&self) -> engine_rapier::terrain::SettlingWork {
        self.loose.settling_work()
    }
    pub fn winner(&self) -> Option<usize> {
        match (self.tanks[0].health > 0.0, self.tanks[1].health > 0.0) {
            (true, false) => Some(0),
            (false, true) => Some(1),
            _ => None,
        }
    }
    pub(crate) fn fighting(&self) -> bool {
        self.tanks.iter().all(|t| t.health > 0.0)
    }
    pub(crate) fn muzzle(&self, physics: &PhysicsWorld, player: usize) -> (Vec2, Vec2) {
        let tank = self.tanks[player];
        let motion = self.motion(physics, tank.body).unwrap();
        let pivot = motion.position + Vec2::Y.rotate_radians(motion.angle) * 0.85;
        (pivot, pivot + tank.direction() * 2.8)
    }
    pub(crate) fn fire(&mut self, physics: &mut PhysicsWorld, player: usize) {
        if self.config.undercut
            || self.config.course.is_some()
            || self.config.drop_height.is_some()
            || self.config.bombardment_seconds > 0
            || !self.firing
            || !self.fighting()
            || self.tick < self.tanks[player].next_shot
            || self.shells.len() >= MAX_SHELLS
        {
            return;
        }
        let tank = self.tanks[player];
        let motion = self.motion(physics, tank.body).unwrap();
        let (pivot, muzzle) = self.muzzle(physics, player);
        // The barrel cannot shoot through the ground or another actor.
        let position = self
            .cast_ray(
                physics,
                pivot,
                muzzle - pivot,
                RayCastOptions {
                    max_distance: (muzzle - pivot).length(),
                    exclude_entity: Some(tank.body.entity),
                    ..Default::default()
                },
            )
            .map_or(muzzle, |hit| hit.point);
        let velocity = tank.direction() * tank.speed + motion.linear_velocity;
        let flight = self.record_launch(physics, player, position, velocity, position != muzzle);
        self.shells.push(Shell {
            owner: player,
            position,
            velocity,
            age: 0,
            flight: Some(flight),
        });
        self.tanks[player].next_shot = self.tick + 75;
        self.tanks[player].shots += 1;
        self.shots += 1;
    }
    pub(crate) fn aim_bot(&mut self, physics: &mut PhysicsWorld, player: usize) {
        if self.calibrated_cpu(player) {
            self.aim_calibrated(physics, player);
            return;
        }
        let origin = self
            .motion(physics, self.tanks[player].body)
            .unwrap()
            .position
            + Vec2::Y;
        let target = self
            .motion(physics, self.tanks[1 - player].body)
            .unwrap()
            .position;
        // A repeatable aiming sweep, not privileged perfect targeting. Ground
        // deformation and intervening loose material can intercept any shot.
        let phase = (self.tanks[player].shots % 5 + self.seed % 5 + player as u64) % 5;
        let angle = (46.0 + phase as f32 * 3.0).to_radians();
        let dx = (target.x - origin.x).abs().max(1.0);
        let dy = target.y - origin.y;
        let denominator = 2.0 * angle.cos().powi(2) * (dx * angle.tan() - dy);
        let speed = if denominator > 0.0 {
            (GRAVITY * dx * dx / denominator).sqrt()
        } else {
            40.0
        };
        self.tanks[player].facing = if target.x >= origin.x { 1.0 } else { -1.0 };
        self.tanks[player].elevation = angle;
        self.tanks[player].speed =
            (speed * (0.93 + phase as f32 * 0.025)).clamp(12.0, self.placement.max_power());
    }
    pub(crate) fn clamp_aim(&mut self) {
        let max_power = self.placement.max_power();
        let tank = &mut self.tanks[self.selected];
        tank.elevation = tank
            .elevation
            .clamp(5.0_f32.to_radians(), 85.0_f32.to_radians());
        tank.speed = tank.speed.clamp(12.0, max_power);
    }
    pub(crate) fn prepare_controls(
        &mut self,
        physics: &mut PhysicsWorld,
        drive: f32,
    ) -> Vec<engine_rapier::world::SurfaceDrive> {
        let goal_axis = if self.config.course.is_some()
            && self.config.course_goal_driving
            && self.config.demo
            && self.config.cpu_driving
        {
            let mut goal = self.course_goal.take().expect("course has a goal");
            let decision = goal.decide_in(self, physics, 0);
            let axis = decision.axis;
            self.driving[0].diagnostics.mode = decision.mode;
            self.course_decision = Some(decision);
            self.course_goal = Some(goal);
            Some((0, axis))
        } else {
            None
        };
        self.prepare_with_drive_override(
            physics,
            drive,
            goal_axis,
            driving::CURRENT_GROUND_CHECKS,
            driving::CURRENT_SUPPORT_CHECKS,
        )
    }

    // The diagnostic fork changes only one drive axis. Physics, controller
    // history, shot scheduling and the other tank continue from the same clone.
    pub(crate) fn prepare_with_drive_override(
        &mut self,
        physics: &mut PhysicsWorld,
        drive: f32,
        override_axis: Option<(usize, f32)>,
        ground_checks: [driving::GroundCheckFn; 2],
        support_checks: [driving::SupportCheckFn; 2],
    ) -> Vec<engine_rapier::world::SurfaceDrive> {
        self.impact_events.clear();
        let drives =
            self.prepare_driving(physics, drive, override_axis, ground_checks, support_checks);
        if self.config.roof {
            self.advance_roof(physics);
        } else if self.config.hillside {
            self.advance_hillside(physics);
        } else if self.config.drop_height.is_some() {
            if self.tick == 120 {
                physics.set_body_kind(drop_test::BODY, BodyKind::Dynamic, true);
            }
        } else if self.config.undercut {
            if self.tick == 120 {
                self.cut_bank_support(physics);
            }
        } else if self.config.bombardment_seconds > 0 {
            // Health-independent rain exercises the real swept-shell impact
            // path. Later shots encounter the already deformed terrain.
            if self.tick < u64::from(self.config.bombardment_seconds) * u64::from(FIXED_HZ)
                && self.tick.is_multiple_of(120)
                && self.shells.len() < MAX_SHELLS
            {
                let phase = (self.shots % 9 + self.seed % 9) % 9;
                self.shells.push(Shell {
                    owner: (self.shots % 2) as usize,
                    position: Vec2::new(-24.0 + phase as f32 * 6.0, 24.0),
                    velocity: Vec2::new(0.0, -8.0),
                    age: 10,
                    flight: None,
                });
                self.shots += 1;
            }
        } else if self.config.course.is_none() && self.firing && self.fighting() {
            for (player, support_check) in support_checks.into_iter().enumerate() {
                if (self.config.demo || player != self.selected)
                    && self.tick >= self.tanks[player].next_shot
                    && self.bot_allows_fire(player)
                    && (!self.config.cpu_driving
                        || self.ready_to_shoot(physics, player, support_check))
                {
                    self.aim_bot(physics, player);
                    self.fire(physics, player);
                    self.tanks[player].next_shot = self.tick + 300;
                }
            }
        }
        self.loose.capture_impact_motion(
            physics,
            self.terrain.iter().map(|f| (f.assembly.body(), &f.terrain)),
            DT,
        );
        drives
    }
    pub(crate) fn finish_step(&mut self, physics: &mut PhysicsWorld) {
        self.loose.sample_impact_contacts(physics);
        let mut finished = Vec::new();
        let placement = &self.placement;
        let retired = self.retired;
        self.shells.retain_mut(|shell| {
            let movement = shell.velocity * DT + Vec2::new(0.0, -0.5 * GRAVITY * DT * DT);
            let hit = physics.cast_ray(
                placement.point_to_world(shell.position),
                movement,
                RayCastOptions {
                    max_distance: movement.length() * placement.scale,
                    exclude_entity: (shell.age < 8).then_some(self.tanks[shell.owner].body.entity),
                    ..Default::default()
                },
            );
            let hit = hit.map(|mut hit| {
                hit.point = placement.point_from_world(hit.point);
                hit
            });
            let end = hit
                .as_ref()
                .map_or(shell.position + movement, |hit| hit.point);
            if let Some(flight) = &mut shell.flight {
                let target = placement.point_from_world(
                    physics
                        .motion(self.tanks[1 - shell.owner].body)
                        .or(retired[1 - shell.owner])
                        .unwrap()
                        .position,
                );
                flight.observe(shell.position, end, target);
            }
            if let Some(hit) = hit {
                let kind = if hit.collider.entity == self.tanks[1 - shell.owner].body.entity {
                    "opponent"
                } else if hit.collider.entity == self.tanks[shell.owner].body.entity {
                    "self"
                } else {
                    "terrain"
                };
                self.impact_events.push(host::Impact {
                    source: host::ShotSource {
                        id: shell.flight.map_or(0, |f| f.source_id()),
                        owner: self.tanks[shell.owner].body.entity,
                    },
                    collider: hit.collider,
                    point: placement.point_to_world(hit.point),
                    velocity: shell.velocity * placement.scale,
                });
                finished.push((*shell, hit.point, kind, true));
                return false;
            }
            shell.position += movement;
            shell.velocity.y -= GRAVITY * DT;
            shell.age += 1;
            let alive =
                shell.age < SHELL_LIFETIME_TICKS && placement.bounds.contains(shell.position);
            if !alive {
                finished.push((
                    *shell,
                    shell.position,
                    if shell.age >= SHELL_LIFETIME_TICKS {
                        "expired"
                    } else {
                        "out of bounds"
                    },
                    false,
                ));
            }
            alive
        });
        for (shell, point, kind, impact) in finished {
            let health = self.tanks.map(|tank| tank.health);
            if impact {
                if shell.flight.is_some() {
                    self.explode_from(physics, point, Some(shell.owner));
                } else {
                    self.explode(physics, point);
                }
            }
            self.record_outcome(shell, point, kind, health);
        }
        self.slump(physics);
        self.fracture_banks(physics);
        self.fracture_impacts(physics);
        self.settle(physics);
        for (slot, tank) in self.tanks.iter_mut().enumerate() {
            let position = self.placement.point_from_world(
                physics
                    .motion(tank.body)
                    .or(self.retired[slot])
                    .unwrap()
                    .position,
            );
            if position.y < self.placement.bounds.min.y + 5.0 {
                tank.health = 0.0
            }
        }
        self.tick += 1;
        self.flashes.retain(|(_, tick)| self.tick - tick < 22);
    }
}

impl ScorchedState {
    pub(crate) fn muzzle(&self, player: usize) -> (Vec2, Vec2) {
        self.duel.muzzle(&self.physics, player)
    }
    pub(crate) fn fire(&mut self, player: usize) {
        self.duel.fire(&mut self.physics, player)
    }
    #[cfg(test)]
    pub(crate) fn aim_bot(&mut self, player: usize) {
        self.duel.aim_bot(&mut self.physics, player)
    }
}

impl Duel {
    pub(crate) fn observe(&self, physics: &PhysicsWorld) -> Observation {
        let state = self;
        let motion = |body| {
            let m = state
                .motion(physics, body)
                .expect("retained physical entity");
            [
                m.position.x,
                m.position.y,
                m.angle,
                m.linear_velocity.x,
                m.linear_velocity.y,
                m.angular_velocity,
            ]
        };
        let fields: Vec<_> = state
            .terrain
            .iter()
            .map(|b| (b.id.value(), b.terrain.hash(), motion(b.assembly.body())))
            .collect();
        let grains: Vec<_> = state
            .loose
            .iter()
            .map(|g| (g.body(), g.cell(), motion(g.body())))
            .collect();
        let tanks: Vec<_> = state.tanks.iter().map(|t| (t, motion(t.body))).collect();
        let mut payload = serde_json::json!({
            "seed": state.seed, "tick": state.tick, "shape": format!("{:?}", state.config.shape),
            "limit": state.config.max_grains, "demo": state.config.demo, "selected": state.selected,
            "bombardment_seconds": state.config.bombardment_seconds,
            "settling_hash": state.loose.settling_hash(), "next_id": state.next_id,
            "shots": state.shots, "impacts": state.impacts, "rejected": state.rejected_blasts,
            "deposited": state.deposited_cells(), "fields": fields, "grains": grains,
            "tanks": tanks, "shells": state.shells,
        });
        // Preserve existing observations when the experiment is disabled.
        if let Some(course) = state.config.course {
            payload["course"] = serde_json::json!(course);
            // History remains part of replay even while manual/range driving
            // suspends the goal. Only Reset or a new course creates a new goal.
            if state.config.course_goal_driving || state.course_decision.is_some() {
                payload["course_goal_driving"] =
                    serde_json::json!(state.config.course_goal_driving);
                payload["course_goal"] = serde_json::json!(state.course_goal);
                payload["course_decision"] = serde_json::json!(state.course_decision);
            }
        }
        if state.config.roof {
            payload["roof"] = serde_json::json!(true);
        }
        if state.config.hillside {
            payload["hillside"] = serde_json::json!(true);
        }
        if state.config.undercut {
            payload["undercut"] = serde_json::json!(true);
        }
        if let Some(height) = state.config.drop_height {
            payload["drop_height"] = serde_json::json!(height.label());
        }
        if state.config.breakup {
            payload["breakup"] = serde_json::json!(true);
        }
        if state.config.cpu_driving || state.driving.iter().any(|d| d.has_history()) {
            payload["cpu_driving"] = serde_json::json!(state.config.cpu_driving);
            payload["driving"] = serde_json::json!(state.driving);
        }
        if state.config.cpu_footing.iter().any(|enabled| *enabled) {
            payload["cpu_footing"] = serde_json::json!(state.config.cpu_footing);
        }
        if state.config.cpu_policy != [CpuPolicy::Sweep; 2] {
            payload["cpu_policy"] = serde_json::json!(state.config.cpu_policy);
            payload["bots"] = serde_json::json!(state.bots);
            payload["flight_observations"] =
                serde_json::json!(state.shells.iter().map(|s| s.flight).collect::<Vec<_>>());
            payload["shot_observations"] = serde_json::json!(state.shot_observations);
        }
        if state.config.suspension {
            payload["suspension"] = serde_json::json!(true);
            // Keep legacy capture identities; record every changed tuning,
            // including a new default, rather than silently relabeling it.
            if state.config.wheel_tuning != WheelTuning::REFERENCE {
                payload["wheel_tuning"] = serde_json::json!(state.config.wheel_tuning);
            }
            payload["wheel_motion"] = serde_json::json!(
                state
                    .running_gear
                    .iter()
                    .enumerate()
                    .map(|(slot, gear)| {
                        gear.as_ref()
                            .filter(|_| state.retired[slot].is_none())
                            .map(|gear| gear.bodies().map(motion))
                    })
                    .collect::<Vec<_>>()
            );
        }
        Observation {
            payload: serde_json::to_vec(&payload).expect("finite scenario observation"),
        }
    }
}
