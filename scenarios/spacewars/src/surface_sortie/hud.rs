//! Read-only gameplay readouts. The client owns pixel layout; none of these
//! presentation choices affect control, physics, or bot observations.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct HudMeter {
    pub label: &'static str,
    pub fraction: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HudPrompt {
    pub title: String,
    pub detail: String,
    pub progress: Option<f32>,
    pub warning: bool,
}

impl HudPrompt {
    fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            progress: None,
            warning: false,
        }
    }
    fn warning(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            warning: true,
            ..Self::new(title, detail)
        }
    }
    fn progress(mut self, progress: f32) -> Self {
        self.progress = Some(progress.clamp(0.0, 1.0));
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerHud {
    pub player: usize,
    pub color: RenderColor,
    pub mode: &'static str,
    pub health: HudMeter,
    pub resource: Option<HudMeter>,
    pub rounds_loaded: Option<u8>,
    pub note: String,
    pub prompt: Option<HudPrompt>,
}

impl SurfaceSortieState {
    pub fn hud_title(&self) -> Option<String> {
        self.match_clock_label().or_else(|| {
            if let Some(surface) = self.surface_comparison {
                return Some(
                    match surface {
                        engine_terrain::TerrainSurface::Blocks => "STEPS",
                        engine_terrain::TerrainSurface::Contour => "SLOPES",
                        engine_terrain::TerrainSurface::Interpolated => "ROUND",
                    }
                    .into(),
                );
            }
            self.generated_case
                .filter(|_| !self.travel_enabled())
                .map(|case| case.profile.label().into())
        })
    }

    pub fn player_hud(&self, player: usize) -> PlayerHud {
        let observation = self.observation(player);
        let ship = &self.world.ships[self.pilots[player].vehicle.0];
        let on_foot = observation.location == PilotLocation::OnFoot;
        let dead = observation.pilot_vitals.is_some_and(|v| !v.alive());
        let supply = (!on_foot && !dead).then(|| ship.weapon_supply()).flatten();
        let health = if on_foot || dead || ship.form == ShipForm::EscapePod {
            HudMeter {
                label: "Pilot",
                fraction: observation.pilot_vitals.map_or(1.0, |v| v.health / 100.0),
            }
        } else {
            HudMeter {
                label: "Hull",
                fraction: ship.life / ship.life_max.max(1.0),
            }
        };
        let resource = if on_foot {
            self.pilots[player]
                .body
                .as_ref()
                .and_then(|body| body.jetpack())
                .map(|pack| HudMeter {
                    label: "Jetpack",
                    fraction: pack.charge,
                })
        } else {
            supply.map(|s| HudMeter {
                label: "Energy",
                fraction: s.energy_percent / 100.0,
            })
        };
        let note = if dead {
            "Pilot lost".into()
        } else if ship.dead || ship.form == ShipForm::EscapePod {
            "Ship lost".into()
        } else if on_foot {
            format!("Ship {:.0}%", ship.life / ship.life_max.max(1.0) * 100.0)
        } else if let Some(supply) = supply
            .filter(|s| s.laser_recharging || s.reload_progress.is_some() || s.rounds_loaded < 2)
        {
            if supply.laser_recharging {
                "Laser charging".into()
            } else if let Some(progress) = supply.reload_progress {
                format!(
                    "Reload {:.1}s",
                    (1.0 - progress) * weapons::ROUND_RELOAD_SECONDS
                )
            } else {
                "Reload needs 25%".into()
            }
        } else if self.has_material_ground() {
            let flight = self.flight_observation(player);
            format!("{} {:.0} u/s", flight.label(), flight.relative_speed)
        } else {
            observation.landing.phase.label().into()
        };
        PlayerHud {
            player,
            color: render_color(self.world.players[observation.owner.index()].color),
            mode: if dead {
                "DEAD"
            } else if on_foot {
                "ON FOOT"
            } else if ship.form == ShipForm::EscapePod {
                "POD"
            } else {
                "SHIP"
            },
            health,
            resource,
            rounds_loaded: supply.map(|s| s.rounds_loaded),
            note,
            prompt: self.hud_prompt(player, &observation),
        }
    }

    fn hud_prompt(&self, player: usize, o: &SurfaceSortieObservation) -> Option<HudPrompt> {
        if let Some(message) = self.match_result_message() {
            let (title, detail) = message.split_once(" / ").unwrap_or((&message, ""));
            return Some(HudPrompt::new(title, detail));
        }
        if !o.controls_armed {
            return Some(HudPrompt::new(
                "Release controls",
                "Then continue in the new body",
            ));
        }
        if let Some(heat) = o.solar.filter(|h| h.intensity > 0.0) {
            return Some(HudPrompt::warning(
                "Solar heat",
                format!(
                    "Leave the sun: -{:.0}% hull/s",
                    heat.damage_percent_per_second
                ),
            ));
        }
        if let Some(v) = o.pilot_vitals.filter(|v| v.protected_until_tick > o.tick) {
            return Some(HudPrompt::new(
                "Ejection protection",
                format!(
                    "{:.1}s remaining",
                    (v.protected_until_tick - o.tick) as f32 / 60.0
                ),
            ));
        }
        if let Some(recovery) = &o.recovery {
            if recovery.scuttle_progress > 0.0 {
                return Some(
                    HudPrompt::warning("Scuttling ship", "Release to cancel")
                        .progress(recovery.scuttle_progress),
                );
            }
            let ship = &self.world.ships[self.pilots[player].vehicle.0];
            if let Some(righting) =
                self.pilots[player].pod_righting_observation(&self.world.physics, ship, &o.landing)
            {
                if righting.remaining_seconds > 0.0 {
                    return Some(HudPrompt::new("Pod recovery lift", "Turn upright"));
                }
                if righting.eligible {
                    return Some(HudPrompt::new("Tipped pod", "Brake + thrust to lift"));
                }
            }
            if let Some(prompt) = recovery_prompt(recovery) {
                return Some(prompt);
            }
        }
        // Feedback for an attempted action expires after three simulated
        // seconds (and freezes while paused). Unlike ongoing rebuild warnings,
        // old transfer errors must not permanently mask capture or flight.
        let transfer = self.pilots[player]
            .last_transfer_tick
            .filter(|tick| o.tick.saturating_sub(*tick) < 180)
            .and_then(|_| match o.last_transfer {
                TransferResult::ExitBlocked if o.landing.phase == LandingPhase::Landed => Some(
                    HudPrompt::warning("Exit blocked", "Clear the hatch; B to retry"),
                ),
                TransferResult::MustBeSupported if o.location == PilotLocation::OnFoot => Some(
                    HudPrompt::new("Settle at the hatch", "Stand on the ship's planet"),
                ),
                TransferResult::TooFar
                    if o.location == PilotLocation::OnFoot
                        && o.position.distance_to(o.access_position) > BOARDING_RANGE =>
                {
                    Some(HudPrompt::new(
                        "Too far to board",
                        "Return to your ship's hatch",
                    ))
                }
                TransferResult::ShipNotSettled if o.landing.phase != LandingPhase::Landed => Some(
                    HudPrompt::new("Not landed yet", "Land and settle before transfer"),
                ),
                TransferResult::VehicleUnavailable if o.recovery.is_none() => Some(
                    HudPrompt::warning("Vehicle lost", "Restart this experiment"),
                ),
                _ => None,
            });
        if transfer.is_some() {
            return transfer;
        }
        if o.location == PilotLocation::OnFoot {
            if let Some(claim) = &o.planet_claim {
                if let Some(prompt) = claim_prompt(claim) {
                    return Some(prompt);
                }
            } else if let Some(post) = o
                .outpost
                .as_ref()
                .filter(|p| p.capture_status != CaptureStatus::Secured)
            {
                return Some(
                    HudPrompt::new(
                        format!("Outpost {}", post.id.0),
                        post.capture_status.label(),
                    )
                    .progress(post.capture_progress),
                );
            }
            if o.position.distance_to(o.access_position) <= BOARDING_RANGE
                && self.vehicle_accessible(player)
                && o.landing.phase == LandingPhase::Landed
            {
                return Some(HudPrompt::new("At your hatch", "B: board"));
            }
            if let Some(mining) = o.mining.as_ref().filter(|m| m.held) {
                return Some(HudPrompt::new(
                    "Mining",
                    format!("Removed {} cells", mining.removed_cells),
                ));
            }
            return None;
        }
        match o.landing.phase {
            LandingPhase::Landed => Some(HudPrompt::new("Landed", "B: exit at the hatch")),
            LandingPhase::Settling => Some(HudPrompt::new("Settling", "Hold steady to land")),
            LandingPhase::Assisted => Some(HudPrompt::new(
                "Landing assist",
                "Rear first; ease onto both feet",
            )),
            LandingPhase::Flying => None,
        }
    }
}

fn recovery_prompt(r: &SurfaceRecoveryObservation) -> Option<HudPrompt> {
    use SurfaceRecoveryStatus::*;
    Some(match r.status {
        ClearanceBlocked => HudPrompt::warning("Rebuild blocked", "Move to clear ground")
            .progress(r.rebuild_progress),
        HatchBlocked => HudPrompt::warning("Rebuild blocked", "Clear space for the hatch")
            .progress(r.rebuild_progress),
        Rebuilding => HudPrompt::new("Rebuilding ship", "Stand still").progress(r.rebuild_progress),
        NeedBalance => HudPrompt::new("Recover your balance", "Then stand still to rebuild"),
        NeedSettle => HudPrompt::new("Ready to rebuild", "Stand still"),
        NeedSupport => HudPrompt::new("Ship lost", "Reach solid ground to rebuild"),
        // Claim/landing prompts give the actionable next step in these states.
        ShipAvailable | Scuttling | LandPod | NeedOwnedPlanet => return None,
    })
}

fn claim_prompt(claim: &PlanetClaimObservation) -> Option<HudPrompt> {
    use PlanetClaimStatus::*;
    let title = match claim.status {
        Lowering => "Lowering enemy flag",
        Raising => "Raising your flag",
        Contested => "Planet contested",
        ApproachFlag => "Lower the enemy flag",
        NeedSupport | NeedBalance | NeedSettle | Ready => "Claim this planet",
        Aboard | Elsewhere | Secured => return None,
    };
    let mut prompt = HudPrompt::new(title, claim.status.label());
    prompt.warning = claim.status == Contested;
    if matches!(claim.status, Lowering | Raising | Contested) {
        prompt = prompt.progress(claim.progress);
    }
    Some(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_priorities_keep_blocked_rebuilds_visible_and_clear_completed_claims() {
        let mut state = SurfaceSortieScenario::init_expedition(42, 1);
        state.pilots[0].controls_armed = true;
        let mut o = state.observation(0);
        o.location = PilotLocation::OnFoot;
        o.ship_available = false;
        o.controls_armed = true;
        let r = o.recovery.as_mut().unwrap();
        r.status = SurfaceRecoveryStatus::ClearanceBlocked;
        r.rebuild_progress = 1.0;
        o.planet_claim.as_mut().unwrap().status = PlanetClaimStatus::Secured;
        for _ in 0..10 {
            let prompt = state.hud_prompt(0, &o).unwrap();
            assert_eq!(prompt.title, "Rebuild blocked");
            assert_eq!(prompt.detail, "Move to clear ground");
            assert_eq!(prompt.progress, Some(1.0));
            assert!(prompt.warning);
        }
        o.recovery.as_mut().unwrap().status = SurfaceRecoveryStatus::NeedOwnedPlanet;
        o.planet_claim.as_mut().unwrap().status = PlanetClaimStatus::Raising;
        o.planet_claim.as_mut().unwrap().progress = 0.4;
        assert_eq!(state.hud_prompt(0, &o).unwrap().title, "Raising your flag");
        assert_eq!(state.hud_prompt(0, &o).unwrap().progress, Some(0.4));
        o.planet_claim.as_mut().unwrap().status = PlanetClaimStatus::Secured;
        // Not near the hatch: no permanent "secured" or tutorial panel.
        o.position += Vec2::X * 100.0;
        assert_eq!(state.hud_prompt(0, &o), None);
    }

    #[test]
    fn flight_is_quiet_and_landing_transfer_and_scuttle_are_contextual() {
        let state = SurfaceSortieScenario::init_expedition(42, 1);
        let mut o = state.observation(0);
        o.controls_armed = true;
        o.last_transfer = TransferResult::ShipNotSettled;
        o.landing.phase = LandingPhase::Flying;
        assert_eq!(state.hud_prompt(0, &o), None);
        for (phase, title) in [
            (LandingPhase::Assisted, "Landing assist"),
            (LandingPhase::Settling, "Settling"),
            (LandingPhase::Landed, "Landed"),
        ] {
            o.landing.phase = phase;
            assert_eq!(state.hud_prompt(0, &o).unwrap().title, title);
        }
        o.recovery.as_mut().unwrap().scuttle_progress = 0.5;
        assert_eq!(state.hud_prompt(0, &o).unwrap().title, "Scuttling ship");
        o.controls_armed = false;
        assert_eq!(state.hud_prompt(0, &o).unwrap().title, "Release controls");
    }

    #[test]
    fn attempted_transfer_gets_feedback_but_does_not_leave_a_stale_panel() {
        let mut state = SurfaceSortieScenario::init_expedition(42, 1);
        let mut o = state.observation(0);
        o.controls_armed = true;
        o.landing.phase = LandingPhase::Flying;
        o.last_transfer = TransferResult::ShipNotSettled;
        state.pilots[0].last_transfer_tick = Some(o.tick);
        assert_eq!(state.hud_prompt(0, &o).unwrap().title, "Not landed yet");
        o.tick += 179;
        assert!(state.hud_prompt(0, &o).is_some());
        o.tick += 1;
        assert_eq!(state.hud_prompt(0, &o), None);
    }

    #[test]
    fn readouts_follow_the_active_body_without_mutating_the_simulation() {
        let mut state = SurfaceSortieScenario::init_expedition(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        for _ in 0..240 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        let ship = state.player_hud(0);
        assert_eq!(ship.mode, "SHIP");
        assert_eq!(ship.health.label, "Hull");
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            dt,
        );
        assert_eq!(state.location(0), PilotLocation::OnFoot);
        let before = SurfaceSortieScenario::observe(&state);
        let foot = state.player_hud(0);
        assert_eq!(foot.mode, "ON FOOT");
        assert_eq!(foot.health.label, "Pilot");
        assert_eq!(foot.rounds_loaded, None);
        assert_eq!(
            SurfaceSortieScenario::observe(&state).payload,
            before.payload
        );
        state.world.ships[0].dead = true;
        assert_eq!(state.player_hud(0).note, "Ship lost");

        let mut state = SurfaceSortieScenario::init_material_match(42);
        state.world.ships[0].change_to_escape_pod();
        let pod = state.player_hud(0);
        assert_eq!(pod.mode, "POD");
        assert_eq!(pod.health.label, "Pilot");
        assert_eq!(pod.note, "Ship lost");
        assert_eq!(pod.rounds_loaded, None);
    }
}
