//! Contact graph rooted in real terrain. Actors are obstacles, not transfer
//! destinations or shortcuts for assigning a pile to a terrain body.
use super::*;
use std::collections::VecDeque;

pub(super) struct ContactGraph {
    pub neighbors: Vec<BTreeSet<usize>>,
    pub roots: Vec<BTreeSet<BodyId>>,
    pub destination: Vec<Option<BodyId>>,
    positions: Vec<Vec2>,
    sizes: Vec<f32>,
}

impl ContactGraph {
    pub fn new(
        world: &PhysicsWorld,
        grains: &[TerrainGrain],
        fields: &BTreeMap<BodyId, TerrainBodyMut<'_>>,
    ) -> Self {
        let ids: BTreeMap<_, _> = grains
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id(), i))
            .collect();
        let mut graph = Self {
            neighbors: vec![BTreeSet::new(); grains.len()],
            roots: vec![BTreeSet::new(); grains.len()],
            destination: vec![None; grains.len()],
            positions: grains
                .iter()
                .map(|g| world.motion(g.body()).unwrap().position)
                .collect(),
            sizes: grains.iter().map(|g| g.cell_size()).collect(),
        };
        for (i, grain) in grains.iter().enumerate() {
            let position = graph.positions[i];
            for contact in world.surface_contacts(grain.collider()) {
                if contact.separation > grain.cell_size() * 0.05 {
                    continue;
                }
                if let Some(&j) = ids.get(&contact.collider.entity) {
                    let other = &grains[j];
                    let other_position = graph.positions[j];
                    if position.distance_to(other_position)
                        <= grain.radius() + other.radius() + grain.cell_size() * 0.05
                    {
                        graph.neighbors[i].insert(j);
                        graph.neighbors[j].insert(i);
                    }
                } else if let Some(body) = world.collider_body(contact.collider)
                    && let Some(field) = fields.get(&body)
                {
                    let motion = world.motion(body).expect("terrain body");
                    let local = (position - motion.position).rotate_radians(-motion.angle);
                    let point = contact.local_surface;
                    if local.distance_to(point.position) <= grain.cell_size() * 0.7
                        && field
                            .geometry
                            .contact_cell(field.terrain, point.position, point.normal)
                            .is_some()
                    {
                        graph.roots[i].insert(body);
                    }
                }
            }
        }
        // Stable multi-source breadth-first traversal prefers the closest root;
        // body/grain identity breaks equal-depth ties independently of Rapier handles.
        let mut roots: Vec<_> = graph
            .roots
            .iter()
            .enumerate()
            .flat_map(|(i, roots)| roots.iter().map(move |&body| (body, grains[i].id(), i)))
            .collect();
        roots.sort();
        let mut queue = VecDeque::new();
        for (body, _, i) in roots {
            if graph.destination[i].is_none() {
                graph.destination[i] = Some(body);
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let body = graph.destination[i];
            for &j in &graph.neighbors[i] {
                if graph.destination[j].is_none() {
                    graph.destination[j] = body;
                    queue.push_back(j);
                }
            }
        }
        graph
    }

    pub fn grounded(&self, ready: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut grounded = BTreeSet::new();
        let mut queue = VecDeque::new();
        for &i in ready {
            if self.destination[i].is_some_and(|body| self.roots[i].contains(&body)) {
                grounded.insert(i);
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            for &j in &self.neighbors[i] {
                if ready.contains(&j)
                    && self.destination[j] == self.destination[i]
                    && grounded.insert(j)
                {
                    queue.push_back(j);
                }
            }
        }
        grounded
    }

    pub fn group(
        &self,
        seed: usize,
        ready: &BTreeSet<usize>,
        attempted: &BTreeSet<usize>,
        limit: usize,
    ) -> Vec<usize> {
        let mut seen = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        let mut group = Vec::new();
        while let Some(i) = queue.pop_front() {
            if !ready.contains(&i)
                || attempted.contains(&i)
                || self.destination[i] != self.destination[seed]
            {
                continue;
            }
            group.push(i);
            if group.len() == limit {
                break;
            }
            // Small gaps between supported proxies still require coordinated
            // placement. Scan only on an attempted group, not every physics tick.
            for &j in ready {
                if self.positions[i].distance_to(self.positions[j]) <= self.sizes[i] * 2.5
                    && seen.insert(j)
                {
                    queue.push_back(j);
                }
            }
        }
        group
    }
}
