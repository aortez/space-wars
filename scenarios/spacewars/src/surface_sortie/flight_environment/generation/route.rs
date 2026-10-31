use super::*;
use std::{cmp::Reverse, collections::BinaryHeap};

/// Every accepted edge is a continuous capsule query. Grid search failure is
/// conservative: a narrow or off-grid physical route may still exist.
pub(super) fn find(
    start: Vec2,
    goal: Vec2,
    center: Vec2,
    radius: f32,
    clear: &impl Fn(Vec2, Vec2) -> bool,
) -> Vec<Vec2> {
    if clear(start, goal) {
        return vec![start, goal];
    }
    let side = (2.0 * radius / GRID_SPACING).ceil() as usize + 1;
    let origin = center - Vec2::new(radius, radius);
    let point = |i: usize| origin + Vec2::new((i % side) as f32, (i / side) as f32) * GRID_SPACING;
    let mut cost = vec![u32::MAX; side * side];
    let mut parent = vec![usize::MAX; side * side];
    let heuristic = |p: Vec2| (p.distance_to(goal) / GRID_SPACING * 1000.0) as u32;
    let mut queue = BinaryHeap::new();
    let nearest_x = ((start.x - origin.x) / GRID_SPACING).round() as i32;
    let nearest_y = ((start.y - origin.y) / GRID_SPACING).round() as i32;
    for y in nearest_y - 1..=nearest_y + 1 {
        for x in nearest_x - 1..=nearest_x + 1 {
            if x < 0 || y < 0 || x >= side as i32 || y >= side as i32 {
                continue;
            }
            let i = y as usize * side + x as usize;
            if clear(start, point(i)) {
                cost[i] = (start.distance_to(point(i)) / GRID_SPACING * 1000.0).ceil() as u32;
                queue.push((
                    Reverse(cost[i] + heuristic(point(i))),
                    Reverse(cost[i]),
                    Reverse(i),
                ));
            }
        }
    }
    while let Some((_, Reverse(g), Reverse(i))) = queue.pop() {
        if cost[i] != g {
            continue;
        }
        let p = point(i);
        if p.distance_to(goal) <= GRID_SPACING * 2.0 && clear(p, goal) {
            let mut route = vec![goal, p];
            let mut previous = parent[i];
            while previous != usize::MAX {
                route.push(point(previous));
                previous = parent[previous];
            }
            route.push(start);
            route.reverse();
            // Preserve a checked geometric witness while removing grid zigzags.
            let mut smooth = vec![start];
            let mut current = 0;
            while current + 1 < route.len() {
                let next = (current + 1..route.len())
                    .rev()
                    .find(|&n| clear(route[current], route[n]))
                    .unwrap();
                smooth.push(route[next]);
                current = next;
            }
            return smooth;
        }
        let x = (i % side) as i32;
        let y = (i / side) as i32;
        for dy in -1_i32..=1 {
            for dx in -1_i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= side as i32 || ny >= side as i32 {
                    continue;
                }
                let n = ny as usize * side + nx as usize;
                let next = g + if dx == 0 || dy == 0 { 1000 } else { 1415 };
                if next < cost[n] && clear(p, point(n)) {
                    cost[n] = next;
                    parent[n] = i;
                    queue.push((
                        Reverse(next + heuristic(point(n))),
                        Reverse(next),
                        Reverse(n),
                    ));
                }
            }
        }
    }
    Vec::new()
}

pub(super) fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let squared = d.length_squared();
    let t = if squared == 0.0 {
        0.0
    } else {
        ((p - a).x * d.x + (p - a).y * d.y) / squared
    }
    .clamp(0.0, 1.0);
    p.distance_to(a + d * t)
}
