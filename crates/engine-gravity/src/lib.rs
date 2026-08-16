//! Deterministic exact and Barnes-Hut many-body gravity.
//!
//! The solver is deliberately independent of any rigid-body implementation.
//! Scenarios provide stable participants, then apply the resulting per-tick
//! velocity deltas to Rapier bodies or lightweight scenario state.

use std::{
    fmt,
    time::{Duration, Instant},
};

use engine_core::Vec2;

const DEFAULT_LEAF_CAPACITY: usize = 3;
const DEFAULT_MAX_DEPTH: u16 = 16;
const MIN_ROOT_HALF_EXTENT: f64 = 1.0;
const ROOT_PADDING: f64 = 1.0e-9;
const NO_NODE: u32 = u32::MAX;

/// Stable identity used to exclude a body from its own gravity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GravityId(u64);

impl GravityId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// How a participant contributes when Barnes-Hut gravity is selected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GravitySourcePolicy {
    /// Include the source in the hierarchical approximation.
    #[default]
    Hierarchical,
    /// Evaluate this source exactly for every target.
    ///
    /// This is useful for a small number of dominant suns or planets whose
    /// near-field behavior should not be approximated.
    Direct,
}

/// One point-mass gravity source and/or target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityParticipant {
    pub id: GravityId,
    pub position: Vec2,
    /// Gravitational source mass. Zero makes this a target only.
    pub source_mass: f32,
    /// Multiplier applied to this participant's response. Zero makes it a
    /// source only; one gives ordinary mass-independent gravitational
    /// acceleration.
    pub response_scale: f32,
    pub source_policy: GravitySourcePolicy,
}

impl GravityParticipant {
    /// Create an ordinary dynamic mass that both attracts and is attracted.
    pub const fn dynamic(id: GravityId, position: Vec2, mass: f32) -> Self {
        Self {
            id,
            position,
            source_mass: mass,
            response_scale: 1.0,
            source_policy: GravitySourcePolicy::Hierarchical,
        }
    }

    /// Create a scripted mass that attracts other objects but does not respond.
    pub const fn direct_source(id: GravityId, position: Vec2, mass: f32) -> Self {
        Self {
            id,
            position,
            source_mass: mass,
            response_scale: 0.0,
            source_policy: GravitySourcePolicy::Direct,
        }
    }

    /// Create an object affected by gravity without making it a source.
    pub const fn target(id: GravityId, position: Vec2, response_scale: f32) -> Self {
        Self {
            id,
            position,
            source_mass: 0.0,
            response_scale,
            source_policy: GravitySourcePolicy::Hierarchical,
        }
    }
}

/// Gravity algorithm selected for one fixed-timestep solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GravityBackend {
    /// Symmetric all-pairs evaluation. This is the correctness oracle.
    Exact,
    /// Barnes-Hut monopole approximation with an opening angle.
    BarnesHut { theta: f32 },
}

/// Parameters for one fixed-timestep gravity solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityConfig {
    pub backend: GravityBackend,
    /// Plummer softening length in world units.
    pub softening: f32,
    /// Per-step interaction scale. For physical units this is `G * dt`; legacy
    /// fixed-timestep scenarios can supply their existing per-tick constant.
    pub interaction_scale: f32,
}

impl Default for GravityConfig {
    fn default() -> Self {
        Self {
            backend: GravityBackend::BarnesHut { theta: 0.7 },
            softening: 1.0e-6,
            interaction_scale: 1.0,
        }
    }
}

/// Per-participant result in the same order as the solver input.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GravityOutput {
    pub id: GravityId,
    pub velocity_delta: Vec2,
}

/// Timings and structural counts from the latest solve.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GravityStepMetrics {
    pub validation_time: Duration,
    pub build_time: Duration,
    pub aggregation_time: Duration,
    pub traversal_time: Duration,
    pub participant_count: usize,
    pub source_count: usize,
    pub hierarchical_source_count: usize,
    pub direct_source_count: usize,
    pub target_count: usize,
    pub node_count: usize,
    pub exact_interactions: u64,
    pub approximations: u64,
    pub applied_sources: u64,
}

/// Accuracy summary comparing candidate outputs with an exact oracle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GravityErrorMetrics {
    pub normalized_rms: f64,
    pub p95_relative: f64,
    pub maximum_relative: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GravityError {
    DuplicateId(GravityId),
    InvalidPosition(GravityId),
    InvalidSourceMass(GravityId),
    InvalidResponseScale(GravityId),
    InvalidSoftening,
    InvalidInteractionScale,
    InvalidTheta,
    OutputIdentityMismatch,
}

impl fmt::Display for GravityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(formatter, "duplicate gravity id {}", id.value()),
            Self::InvalidPosition(id) => {
                write!(
                    formatter,
                    "gravity participant {} has a non-finite position",
                    id.value()
                )
            }
            Self::InvalidSourceMass(id) => write!(
                formatter,
                "gravity participant {} has an invalid source mass",
                id.value()
            ),
            Self::InvalidResponseScale(id) => write!(
                formatter,
                "gravity participant {} has an invalid response scale",
                id.value()
            ),
            Self::InvalidSoftening => write!(
                formatter,
                "gravity softening must be finite and non-negative"
            ),
            Self::InvalidInteractionScale => {
                write!(formatter, "gravity interaction scale must be finite")
            }
            Self::InvalidTheta => write!(
                formatter,
                "Barnes-Hut theta must be finite and non-negative"
            ),
            Self::OutputIdentityMismatch => {
                write!(
                    formatter,
                    "gravity outputs do not describe the same participant order"
                )
            }
        }
    }
}

impl std::error::Error for GravityError {}

#[derive(Debug, Clone, Copy, Default)]
struct Acceleration64 {
    x: f64,
    y: f64,
}

impl Acceleration64 {
    fn add_scaled(&mut self, dx: f64, dy: f64, scale: f64) {
        self.x += dx * scale;
        self.y += dy * scale;
    }
}

#[derive(Debug, Clone, Copy)]
struct Node {
    center_x: f64,
    center_y: f64,
    half_extent: f64,
    parent: u32,
    first_child: u32,
    first_source: u32,
    source_count: u32,
    aggregate_mass: f64,
    aggregate_center_x: f64,
    aggregate_center_y: f64,
    path_marker: u32,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_y: 0.0,
            half_extent: 0.0,
            parent: NO_NODE,
            first_child: NO_NODE,
            first_source: 0,
            source_count: 0,
            aggregate_mass: 0.0,
            aggregate_center_x: 0.0,
            aggregate_center_y: 0.0,
            path_marker: 0,
        }
    }
}

impl Node {
    fn leaf(self) -> bool {
        self.first_child == NO_NODE
    }

    fn size_squared(self) -> f64 {
        let size = self.half_extent * 2.0;
        size * size
    }
}

/// Reusable exact/Barnes-Hut workspace.
#[derive(Debug, Clone, Default)]
pub struct GravitySolver {
    stable_order: Vec<usize>,
    hierarchical_sources: Vec<usize>,
    direct_sources: Vec<usize>,
    source_order: Vec<usize>,
    partition_scratch: Vec<usize>,
    source_leaf: Vec<u32>,
    nodes: Vec<Node>,
    traversal_stack: Vec<u32>,
    accumulators: Vec<Acceleration64>,
    outputs: Vec<GravityOutput>,
    mass_traversal_marker: u32,
    metrics: GravityStepMetrics,
}

impl GravitySolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reserve(&mut self, participants: usize, nodes: usize) {
        self.stable_order.reserve(participants);
        self.hierarchical_sources.reserve(participants);
        self.direct_sources.reserve(participants);
        self.source_order.reserve(participants);
        self.partition_scratch.reserve(participants);
        self.source_leaf.reserve(participants);
        self.accumulators.reserve(participants);
        self.outputs.reserve(participants);
        self.nodes.reserve(nodes);
        self.traversal_stack.reserve(nodes);
    }

    /// Solve one fixed-timestep gravity update.
    pub fn solve(
        &mut self,
        participants: &[GravityParticipant],
        config: GravityConfig,
    ) -> Result<&[GravityOutput], GravityError> {
        self.metrics = GravityStepMetrics::default();
        self.prepare_output(participants);

        let validation_started = Instant::now();
        self.validate_and_order(participants, config)?;
        self.metrics.validation_time = validation_started.elapsed();
        self.metrics.participant_count = participants.len();
        self.metrics.source_count = self.hierarchical_sources.len() + self.direct_sources.len();
        self.metrics.hierarchical_source_count = self.hierarchical_sources.len();
        self.metrics.direct_source_count = self.direct_sources.len();
        self.metrics.target_count = self
            .stable_order
            .iter()
            .filter(|&&index| participants[index].response_scale > 0.0)
            .count();

        match config.backend {
            GravityBackend::Exact => self.solve_exact(participants, config),
            GravityBackend::BarnesHut { theta } => {
                self.solve_barnes_hut(participants, config, theta)
            }
        }

        for (output, acceleration) in self.outputs.iter_mut().zip(&self.accumulators) {
            output.velocity_delta = Vec2::new(acceleration.x as f32, acceleration.y as f32);
        }
        Ok(&self.outputs)
    }

    pub fn metrics(&self) -> GravityStepMetrics {
        self.metrics
    }

    fn prepare_output(&mut self, participants: &[GravityParticipant]) {
        self.accumulators.clear();
        self.accumulators
            .resize(participants.len(), Acceleration64::default());
        self.outputs.clear();
        self.outputs
            .extend(participants.iter().map(|participant| GravityOutput {
                id: participant.id,
                velocity_delta: Vec2::ZERO,
            }));
    }

    fn validate_and_order(
        &mut self,
        participants: &[GravityParticipant],
        config: GravityConfig,
    ) -> Result<(), GravityError> {
        if !config.softening.is_finite() || config.softening < 0.0 {
            return Err(GravityError::InvalidSoftening);
        }
        if !config.interaction_scale.is_finite() {
            return Err(GravityError::InvalidInteractionScale);
        }
        if let GravityBackend::BarnesHut { theta } = config.backend
            && (!theta.is_finite() || theta < 0.0)
        {
            return Err(GravityError::InvalidTheta);
        }

        self.stable_order.clear();
        self.stable_order.extend(0..participants.len());
        self.stable_order
            .sort_unstable_by_key(|&index| participants[index].id);

        self.hierarchical_sources.clear();
        self.direct_sources.clear();
        let mut previous = None;
        for &index in &self.stable_order {
            let participant = participants[index];
            if previous == Some(participant.id) {
                return Err(GravityError::DuplicateId(participant.id));
            }
            previous = Some(participant.id);
            if !participant.position.x.is_finite() || !participant.position.y.is_finite() {
                return Err(GravityError::InvalidPosition(participant.id));
            }
            if !participant.source_mass.is_finite() || participant.source_mass < 0.0 {
                return Err(GravityError::InvalidSourceMass(participant.id));
            }
            if !participant.response_scale.is_finite() || participant.response_scale < 0.0 {
                return Err(GravityError::InvalidResponseScale(participant.id));
            }
            if participant.source_mass > 0.0 {
                match participant.source_policy {
                    GravitySourcePolicy::Hierarchical => {
                        self.hierarchical_sources.push(index);
                    }
                    GravitySourcePolicy::Direct => self.direct_sources.push(index),
                }
            }
        }
        Ok(())
    }

    fn solve_exact(&mut self, participants: &[GravityParticipant], config: GravityConfig) {
        let traversal_started = Instant::now();
        let softening_squared = f64::from(config.softening).powi(2);
        let interaction_scale = f64::from(config.interaction_scale);

        for left_order in 0..self.stable_order.len() {
            let left_index = self.stable_order[left_order];
            let left = participants[left_index];
            for &right_index in &self.stable_order[left_order + 1..] {
                let right = participants[right_index];
                let dx = f64::from(right.position.x) - f64::from(left.position.x);
                let dy = f64::from(right.position.y) - f64::from(left.position.y);
                let distance_without_softening = dx * dx + dy * dy;
                if distance_without_softening == 0.0 {
                    continue;
                }
                let distance_squared = distance_without_softening + softening_squared;
                let inverse_distance = distance_squared.sqrt().recip();
                let base_scale = interaction_scale * inverse_distance / distance_squared;
                let mut applied = false;

                if left.response_scale > 0.0 && right.source_mass > 0.0 {
                    let scale =
                        base_scale * f64::from(right.source_mass) * f64::from(left.response_scale);
                    self.accumulators[left_index].add_scaled(dx, dy, scale);
                    self.metrics.applied_sources += 1;
                    applied = true;
                }
                if right.response_scale > 0.0 && left.source_mass > 0.0 {
                    let scale =
                        base_scale * f64::from(left.source_mass) * f64::from(right.response_scale);
                    self.accumulators[right_index].add_scaled(-dx, -dy, scale);
                    self.metrics.applied_sources += 1;
                    applied = true;
                }
                if applied {
                    self.metrics.exact_interactions += 1;
                }
            }
        }
        self.metrics.traversal_time = traversal_started.elapsed();
    }

    fn solve_barnes_hut(
        &mut self,
        participants: &[GravityParticipant],
        config: GravityConfig,
        theta: f32,
    ) {
        let build_started = Instant::now();
        self.build_tree(participants);
        self.metrics.build_time = build_started.elapsed();
        self.metrics.node_count = self.nodes.len();

        let aggregation_started = Instant::now();
        self.aggregate_mass(participants);
        self.metrics.aggregation_time = aggregation_started.elapsed();

        let traversal_started = Instant::now();
        let theta_squared = f64::from(theta).powi(2);
        let softening_squared = f64::from(config.softening).powi(2);
        let interaction_scale = f64::from(config.interaction_scale);
        for target_order_index in 0..self.stable_order.len() {
            let target_index = self.stable_order[target_order_index];
            let target = participants[target_index];
            if target.response_scale == 0.0 {
                continue;
            }
            self.apply_direct_sources(
                participants,
                target_index,
                softening_squared,
                interaction_scale,
            );
            self.apply_tree_acceleration(
                participants,
                target_index,
                theta_squared,
                softening_squared,
                interaction_scale,
            );
        }
        self.metrics.traversal_time = traversal_started.elapsed();
    }

    fn build_tree(&mut self, participants: &[GravityParticipant]) {
        self.nodes.clear();
        self.source_order.clear();
        self.source_order
            .extend(self.hierarchical_sources.iter().copied());
        self.source_leaf.clear();
        self.source_leaf.resize(participants.len(), NO_NODE);
        self.partition_scratch.clear();
        self.partition_scratch.resize(self.source_order.len(), 0);

        if self.source_order.is_empty() {
            return;
        }

        let (center_x, center_y, half_extent) = root_bounds(participants, &self.source_order);
        self.nodes.push(Node {
            center_x,
            center_y,
            half_extent,
            ..Node::default()
        });
        self.build_node(participants, 0, 0, self.source_order.len(), 0);
    }

    fn build_node(
        &mut self,
        participants: &[GravityParticipant],
        node_index: u32,
        start: usize,
        end: usize,
        depth: u16,
    ) {
        let count = end - start;
        {
            let node = &mut self.nodes[node_index as usize];
            node.first_source = start as u32;
            node.source_count = count as u32;
        }
        if count <= DEFAULT_LEAF_CAPACITY || depth >= DEFAULT_MAX_DEPTH {
            for &participant_index in &self.source_order[start..end] {
                self.source_leaf[participant_index] = node_index;
            }
            return;
        }

        let node = self.nodes[node_index as usize];
        let mut counts = [0_usize; 4];
        for &participant_index in &self.source_order[start..end] {
            counts[quadrant(
                participants[participant_index].position,
                node.center_x,
                node.center_y,
            )] += 1;
        }

        let offsets = [
            start,
            start + counts[0],
            start + counts[0] + counts[1],
            start + counts[0] + counts[1] + counts[2],
        ];
        let mut cursors = offsets;
        for &participant_index in &self.source_order[start..end] {
            let quadrant = quadrant(
                participants[participant_index].position,
                node.center_x,
                node.center_y,
            );
            self.partition_scratch[cursors[quadrant]] = participant_index;
            cursors[quadrant] += 1;
        }
        self.source_order[start..end].copy_from_slice(&self.partition_scratch[start..end]);

        let first_child = self.nodes.len() as u32;
        self.nodes[node_index as usize].first_child = first_child;
        let child_half = node.half_extent * 0.5;
        for child in 0..4 {
            let (offset_x, offset_y) = quadrant_offset(child, child_half);
            self.nodes.push(Node {
                center_x: node.center_x + offset_x,
                center_y: node.center_y + offset_y,
                half_extent: child_half,
                parent: node_index,
                ..Node::default()
            });
        }

        for child in 0..4 {
            let child_start = offsets[child];
            let child_end = child_start + counts[child];
            self.build_node(
                participants,
                first_child + child as u32,
                child_start,
                child_end,
                depth + 1,
            );
        }
    }

    fn aggregate_mass(&mut self, participants: &[GravityParticipant]) {
        for node_index in (0..self.nodes.len()).rev() {
            let node = self.nodes[node_index];
            let (mass, weighted_x, weighted_y) = if node.leaf() {
                let start = node.first_source as usize;
                let end = start + node.source_count as usize;
                self.source_order[start..end].iter().fold(
                    (0.0, 0.0, 0.0),
                    |(mass, weighted_x, weighted_y), &participant_index| {
                        let source = participants[participant_index];
                        let source_mass = f64::from(source.source_mass);
                        (
                            mass + source_mass,
                            weighted_x + f64::from(source.position.x) * source_mass,
                            weighted_y + f64::from(source.position.y) * source_mass,
                        )
                    },
                )
            } else {
                (0..4).fold((0.0, 0.0, 0.0), |(mass, weighted_x, weighted_y), child| {
                    let child_node = self.nodes[node.first_child as usize + child];
                    (
                        mass + child_node.aggregate_mass,
                        weighted_x + child_node.aggregate_center_x * child_node.aggregate_mass,
                        weighted_y + child_node.aggregate_center_y * child_node.aggregate_mass,
                    )
                })
            };

            let current = &mut self.nodes[node_index];
            current.aggregate_mass = mass;
            if mass > 0.0 {
                current.aggregate_center_x = weighted_x / mass;
                current.aggregate_center_y = weighted_y / mass;
            } else {
                current.aggregate_center_x = current.center_x;
                current.aggregate_center_y = current.center_y;
            }
        }
    }

    fn apply_direct_sources(
        &mut self,
        participants: &[GravityParticipant],
        target_index: usize,
        softening_squared: f64,
        interaction_scale: f64,
    ) {
        let target = participants[target_index];
        for source_order_index in 0..self.direct_sources.len() {
            let source_index = self.direct_sources[source_order_index];
            if source_index == target_index {
                continue;
            }
            let source = participants[source_index];
            if add_source_acceleration(
                &mut self.accumulators[target_index],
                target,
                source,
                softening_squared,
                interaction_scale,
            ) {
                self.metrics.exact_interactions += 1;
                self.metrics.applied_sources += 1;
            }
        }
    }

    fn apply_tree_acceleration(
        &mut self,
        participants: &[GravityParticipant],
        target_index: usize,
        theta_squared: f64,
        softening_squared: f64,
        interaction_scale: f64,
    ) {
        if self.nodes.is_empty() {
            return;
        }

        let marker = self.next_marker();
        let mut node_index = self.source_leaf[target_index];
        while node_index != NO_NODE {
            let node = &mut self.nodes[node_index as usize];
            node.path_marker = marker;
            node_index = node.parent;
        }

        self.traversal_stack.clear();
        self.traversal_stack.push(0);
        let target = participants[target_index];
        while let Some(node_index) = self.traversal_stack.pop() {
            let node = self.nodes[node_index as usize];
            if node.aggregate_mass == 0.0 {
                continue;
            }

            if node.leaf() {
                let start = node.first_source as usize;
                let end = start + node.source_count as usize;
                for source_order_index in start..end {
                    let source_index = self.source_order[source_order_index];
                    if source_index == target_index {
                        continue;
                    }
                    if add_source_acceleration(
                        &mut self.accumulators[target_index],
                        target,
                        participants[source_index],
                        softening_squared,
                        interaction_scale,
                    ) {
                        self.metrics.exact_interactions += 1;
                        self.metrics.applied_sources += 1;
                    }
                }
                continue;
            }

            let dx = node.aggregate_center_x - f64::from(target.position.x);
            let dy = node.aggregate_center_y - f64::from(target.position.y);
            let distance_without_softening = dx * dx + dy * dy;
            if node.path_marker != marker
                && distance_without_softening > 0.0
                && node.size_squared() < theta_squared * distance_without_softening
            {
                let distance_squared = distance_without_softening + softening_squared;
                let inverse_distance = distance_squared.sqrt().recip();
                let scale = interaction_scale
                    * node.aggregate_mass
                    * f64::from(target.response_scale)
                    * inverse_distance
                    / distance_squared;
                self.accumulators[target_index].add_scaled(dx, dy, scale);
                self.metrics.approximations += 1;
                self.metrics.applied_sources += 1;
                continue;
            }

            for child in (0..4).rev() {
                self.traversal_stack.push(node.first_child + child);
            }
        }
    }

    fn next_marker(&mut self) -> u32 {
        self.mass_traversal_marker = self.mass_traversal_marker.wrapping_add(1);
        if self.mass_traversal_marker == 0 {
            for node in &mut self.nodes {
                node.path_marker = 0;
            }
            self.mass_traversal_marker = 1;
        }
        self.mass_traversal_marker
    }
}

/// Compare candidate deltas against an exact result produced from the same
/// participant order.
pub fn compare_outputs(
    reference: &[GravityOutput],
    candidate: &[GravityOutput],
) -> Result<GravityErrorMetrics, GravityError> {
    if reference.len() != candidate.len()
        || reference
            .iter()
            .zip(candidate)
            .any(|(reference, candidate)| reference.id != candidate.id)
    {
        return Err(GravityError::OutputIdentityMismatch);
    }
    if reference.is_empty() {
        return Ok(GravityErrorMetrics::default());
    }

    let mut squared_error_sum = 0.0;
    let mut squared_reference_sum = 0.0;
    let mut relative = Vec::with_capacity(reference.len());
    for (reference, candidate) in reference.iter().zip(candidate) {
        let reference_x = f64::from(reference.velocity_delta.x);
        let reference_y = f64::from(reference.velocity_delta.y);
        let error_x = f64::from(candidate.velocity_delta.x) - reference_x;
        let error_y = f64::from(candidate.velocity_delta.y) - reference_y;
        let error_squared = error_x * error_x + error_y * error_y;
        let reference_squared = reference_x * reference_x + reference_y * reference_y;
        squared_error_sum += error_squared;
        squared_reference_sum += reference_squared;
        relative.push(error_squared.sqrt() / reference_squared.sqrt().max(f64::EPSILON));
    }
    relative.sort_unstable_by(f64::total_cmp);
    let p95_index = ((relative.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(relative.len() - 1);

    Ok(GravityErrorMetrics {
        normalized_rms: (squared_error_sum / squared_reference_sum.max(f64::EPSILON)).sqrt(),
        p95_relative: relative[p95_index],
        maximum_relative: *relative.last().unwrap_or(&0.0),
    })
}

fn root_bounds(participants: &[GravityParticipant], source_order: &[usize]) -> (f64, f64, f64) {
    let mut minimum_x = f64::INFINITY;
    let mut maximum_x = f64::NEG_INFINITY;
    let mut minimum_y = f64::INFINITY;
    let mut maximum_y = f64::NEG_INFINITY;
    for &index in source_order {
        let position = participants[index].position;
        minimum_x = minimum_x.min(f64::from(position.x));
        maximum_x = maximum_x.max(f64::from(position.x));
        minimum_y = minimum_y.min(f64::from(position.y));
        maximum_y = maximum_y.max(f64::from(position.y));
    }
    let center_x = (minimum_x + maximum_x) * 0.5;
    let center_y = (minimum_y + maximum_y) * 0.5;
    let half_extent = ((maximum_x - minimum_x).max(maximum_y - minimum_y) * 0.5)
        .max(MIN_ROOT_HALF_EXTENT)
        * (1.0 + ROOT_PADDING);
    (center_x, center_y, half_extent)
}

fn quadrant(position: Vec2, center_x: f64, center_y: f64) -> usize {
    let east = usize::from(f64::from(position.x) >= center_x);
    let south = usize::from(f64::from(position.y) >= center_y);
    south * 2 + east
}

fn quadrant_offset(quadrant: usize, half_extent: f64) -> (f64, f64) {
    let x = if quadrant.is_multiple_of(2) {
        -half_extent
    } else {
        half_extent
    };
    let y = if quadrant < 2 {
        -half_extent
    } else {
        half_extent
    };
    (x, y)
}

fn add_source_acceleration(
    acceleration: &mut Acceleration64,
    target: GravityParticipant,
    source: GravityParticipant,
    softening_squared: f64,
    interaction_scale: f64,
) -> bool {
    let dx = f64::from(source.position.x) - f64::from(target.position.x);
    let dy = f64::from(source.position.y) - f64::from(target.position.y);
    let distance_without_softening = dx * dx + dy * dy;
    if distance_without_softening == 0.0 {
        return false;
    }
    let distance_squared = distance_without_softening + softening_squared;
    let inverse_distance = distance_squared.sqrt().recip();
    let scale = interaction_scale
        * f64::from(source.source_mass)
        * f64::from(target.response_scale)
        * inverse_distance
        / distance_squared;
    acceleration.add_scaled(dx, dy, scale);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(count: usize, seed: u64) -> Vec<GravityParticipant> {
        let side = (count as f64).sqrt().ceil() as usize;
        let mut random = seed;
        (0..count)
            .map(|index| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let jitter_x = ((random >> 40) as f32 / (1_u32 << 24) as f32 - 0.5) * 3.0;
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let jitter_y = ((random >> 40) as f32 / (1_u32 << 24) as f32 - 0.5) * 3.0;
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let mass = 0.5 + (random >> 40) as f32 / (1_u32 << 24) as f32 * 4.5;
                let position = Vec2::new(
                    (index % side) as f32 * 10.0 + jitter_x,
                    (index / side) as f32 * 10.0 + jitter_y,
                );
                GravityParticipant::dynamic(GravityId::new(index as u64 + 1), position, mass)
            })
            .collect()
    }

    fn exact_config() -> GravityConfig {
        GravityConfig {
            backend: GravityBackend::Exact,
            softening: 0.0,
            interaction_scale: 1.0,
        }
    }

    #[test]
    fn exact_two_body_gravity_is_symmetric_by_mass() {
        let participants = [
            GravityParticipant::dynamic(GravityId::new(1), Vec2::new(0.0, 0.0), 2.0),
            GravityParticipant::dynamic(GravityId::new(2), Vec2::new(2.0, 0.0), 4.0),
        ];
        let mut solver = GravitySolver::new();
        let outputs = solver.solve(&participants, exact_config()).unwrap();

        assert_eq!(outputs[0].velocity_delta, Vec2::new(1.0, 0.0));
        assert_eq!(outputs[1].velocity_delta, Vec2::new(-0.5, 0.0));
        assert_eq!(solver.metrics().exact_interactions, 1);
        assert_eq!(solver.metrics().applied_sources, 2);
    }

    #[test]
    fn direct_source_attracts_without_responding() {
        let participants = [
            GravityParticipant::direct_source(GravityId::new(1), Vec2::new(0.0, 0.0), 100.0),
            GravityParticipant::dynamic(GravityId::new(2), Vec2::new(10.0, 0.0), 1.0),
        ];
        let mut solver = GravitySolver::new();
        let outputs = solver
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 0.7 },
                    ..exact_config()
                },
            )
            .unwrap();

        assert_eq!(outputs[0].velocity_delta, Vec2::ZERO);
        assert_eq!(outputs[1].velocity_delta, Vec2::new(-1.0, 0.0));
    }

    #[test]
    fn exact_gravity_conserves_linear_momentum() {
        let participants = fixture(257, 0xC0FF_EE11);
        let mut solver = GravitySolver::new();
        let outputs = solver
            .solve(
                &participants,
                GravityConfig {
                    softening: 0.25,
                    interaction_scale: 0.0005,
                    ..exact_config()
                },
            )
            .unwrap();

        let momentum_delta = participants.iter().zip(outputs).fold(
            Acceleration64::default(),
            |mut total, (participant, output)| {
                total.x += f64::from(participant.source_mass) * f64::from(output.velocity_delta.x);
                total.y += f64::from(participant.source_mass) * f64::from(output.velocity_delta.y);
                total
            },
        );

        assert!(momentum_delta.x.abs() < 2.0e-7, "{momentum_delta:?}");
        assert!(momentum_delta.y.abs() < 2.0e-7, "{momentum_delta:?}");
    }

    #[test]
    fn participant_order_does_not_change_exact_results() {
        let participants = [
            GravityParticipant::dynamic(GravityId::new(3), Vec2::new(3.0, 2.0), 2.0),
            GravityParticipant::dynamic(GravityId::new(1), Vec2::new(-1.0, 0.0), 3.0),
            GravityParticipant::dynamic(GravityId::new(2), Vec2::new(2.0, -4.0), 1.5),
        ];
        let shuffled = [participants[1], participants[2], participants[0]];
        let mut first = GravitySolver::new();
        let mut second = GravitySolver::new();
        let first_outputs = first.solve(&participants, exact_config()).unwrap().to_vec();
        let second_outputs = second.solve(&shuffled, exact_config()).unwrap();

        for output in first_outputs {
            let matching = second_outputs
                .iter()
                .find(|candidate| candidate.id == output.id)
                .unwrap();
            assert_eq!(output.velocity_delta, matching.velocity_delta);
        }
    }

    #[test]
    fn participant_order_does_not_change_barnes_hut_results() {
        let participants = fixture(128, 17);
        let mut shuffled = participants.clone();
        shuffled.reverse();
        let config = GravityConfig {
            backend: GravityBackend::BarnesHut { theta: 0.7 },
            softening: 1.0e-6,
            interaction_scale: 0.0005,
        };
        let mut first = GravitySolver::new();
        let mut second = GravitySolver::new();
        let first_outputs = first.solve(&participants, config).unwrap().to_vec();
        let second_outputs = second.solve(&shuffled, config).unwrap();

        for output in first_outputs {
            let matching = second_outputs
                .iter()
                .find(|candidate| candidate.id == output.id)
                .unwrap();
            assert_eq!(output.velocity_delta, matching.velocity_delta);
        }
    }

    #[test]
    fn barnes_hut_has_bounded_error_against_exact_fixture() {
        let participants = fixture(1_000, 0xA11A_0B1A);
        let mut exact = GravitySolver::new();
        let reference = exact
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::Exact,
                    softening: 1.0e-6,
                    interaction_scale: 0.0005,
                },
            )
            .unwrap()
            .to_vec();
        for (theta, maximum_rms, maximum_p95) in [(0.5, 0.015, 0.08), (0.7, 0.04, 0.1)] {
            let mut approximate = GravitySolver::new();
            let candidate = approximate
                .solve(
                    &participants,
                    GravityConfig {
                        backend: GravityBackend::BarnesHut { theta },
                        softening: 1.0e-6,
                        interaction_scale: 0.0005,
                    },
                )
                .unwrap();
            let error = compare_outputs(&reference, candidate).unwrap();

            assert!(
                error.normalized_rms < maximum_rms,
                "theta {theta}: error {error:?}, metrics {:?}",
                approximate.metrics()
            );
            assert!(
                error.p95_relative < maximum_p95,
                "theta {theta}: error {error:?}, metrics {:?}",
                approximate.metrics()
            );
            assert!(approximate.metrics().applied_sources < 1_000 * 999 / 2);
        }
    }

    #[test]
    fn barnes_hut_never_applies_a_target_containing_node() {
        let mut participants = vec![GravityParticipant::dynamic(
            GravityId::new(1),
            Vec2::ZERO,
            1_000.0,
        )];
        participants.extend(
            (2..=10).map(|id| {
                GravityParticipant::dynamic(GravityId::new(id), Vec2::new(100.0, 0.0), 1.0)
            }),
        );
        let mut exact = GravitySolver::new();
        let reference = exact.solve(&participants, exact_config()).unwrap().to_vec();
        let mut approximate = GravitySolver::new();
        let candidate = approximate
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 100.0 },
                    ..exact_config()
                },
            )
            .unwrap();

        assert_eq!(reference[0], candidate[0]);
        assert!(approximate.metrics().approximations > 0);
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let participants = [
            GravityParticipant::dynamic(GravityId::new(1), Vec2::ZERO, 1.0),
            GravityParticipant::dynamic(GravityId::new(1), Vec2::X, 1.0),
        ];
        let mut solver = GravitySolver::new();
        assert_eq!(
            solver.solve(&participants, exact_config()),
            Err(GravityError::DuplicateId(GravityId::new(1)))
        );
    }

    #[test]
    fn coincident_sources_stop_at_maximum_depth_without_nan() {
        let participants = (0..100)
            .map(|index| {
                GravityParticipant::dynamic(GravityId::new(index + 1), Vec2::new(4.0, -2.0), 1.0)
            })
            .collect::<Vec<_>>();
        let mut solver = GravitySolver::new();
        let outputs = solver
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 0.7 },
                    ..exact_config()
                },
            )
            .unwrap();

        assert!(
            outputs
                .iter()
                .all(|output| output.velocity_delta == Vec2::ZERO)
        );
        assert!(solver.metrics().node_count <= 1 + DEFAULT_MAX_DEPTH as usize * 4);
    }

    #[test]
    fn reusable_solver_handles_population_churn_without_stale_sources() {
        let mut solver = GravitySolver::new();
        let first = fixture(512, 10);
        solver
            .solve(
                &first,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 0.7 },
                    ..exact_config()
                },
            )
            .unwrap();

        let mut second = fixture(197, 20);
        for (index, participant) in second.iter_mut().enumerate() {
            participant.id = GravityId::new(10_000 + index as u64);
        }
        let reused = solver
            .solve(
                &second,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 0.0 },
                    ..exact_config()
                },
            )
            .unwrap()
            .to_vec();
        let mut exact = GravitySolver::new();
        let reference = exact.solve(&second, exact_config()).unwrap();
        let error = compare_outputs(reference, &reused).unwrap();

        assert!(error.normalized_rms < 1.0e-6, "{error:?}");
        assert_eq!(solver.metrics().participant_count, second.len());
        assert_eq!(solver.metrics().source_count, second.len());
    }

    #[test]
    fn target_only_participant_does_not_contribute_mass() {
        let participants = [
            GravityParticipant::direct_source(GravityId::new(1), Vec2::ZERO, 4.0),
            GravityParticipant {
                id: GravityId::new(2),
                position: Vec2::new(2.0, 0.0),
                source_mass: 0.0,
                response_scale: 1.0,
                source_policy: GravitySourcePolicy::Hierarchical,
            },
            GravityParticipant::dynamic(GravityId::new(3), Vec2::new(4.0, 0.0), 2.0),
        ];
        let mut solver = GravitySolver::new();
        let outputs = solver
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 100.0 },
                    ..exact_config()
                },
            )
            .unwrap();

        assert_eq!(outputs[0].velocity_delta, Vec2::ZERO);
        assert_eq!(outputs[1].velocity_delta, Vec2::new(-0.5, 0.0));
        assert_eq!(outputs[2].velocity_delta, Vec2::new(-0.25, 0.0));
        assert_eq!(solver.metrics().source_count, 2);
        assert_eq!(solver.metrics().target_count, 2);
    }

    #[test]
    fn empty_output_comparison_has_zero_error() {
        assert_eq!(
            compare_outputs(&[], &[]).unwrap(),
            GravityErrorMetrics::default()
        );
    }
}
