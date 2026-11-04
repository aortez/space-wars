"""Route choice on the sensed polar map: A* over occupancy cells from the ship to the goal.

The map is the policy observation's own cell grid, 23 radial bands by 64
sectors around the body origin, so the route uses nothing the network could
not see. Cells touched by geometry are blocked, and every cell whose center
lies within the hull margin of a blocked cell center is blocked too. The
result is the first waypoint of the path beyond a short lead distance; the
caller turns it into a heading and keeps its own clearance and speed rules.
"""
import heapq
import math

SECTORS = 64


def cell_center(edges, band, sector):
    """Body-frame center of a cell: sectors run from +x, bands from the radial edges."""
    radius = .5 * (edges[band] + edges[band + 1]); angle = (sector + .5) * 2. * math.pi / SECTORS
    return radius * math.cos(angle), radius * math.sin(angle)


def blocked_cells(cells, edges, hull_margin):
    """Blocked flags per (band, sector), inflated by the hull margin around every touched cell center."""
    bands = len(edges) - 1
    touched = [(b, s) for b in range(bands) for s in range(SECTORS) if cells[b * SECTORS + s][0] > 0. or cells[b * SECTORS + s][1] > 0.]
    blocked = set(touched)
    centers = {(b, s): cell_center(edges, b, s) for b in range(bands) for s in range(SECTORS)}
    touched_centers = [centers[k] for k in touched]
    for key, (x, y) in centers.items():
        if key in blocked:
            continue
        if any((x - tx) ** 2 + (y - ty) ** 2 < hull_margin ** 2 for tx, ty in touched_centers):
            blocked.add(key)
    return blocked, centers


def locate(edges, point):
    """Cell containing a body-frame point, or None beyond the outer edge."""
    radius = math.hypot(*point)
    if radius >= edges[-1]:
        return None
    band = max(0, next(i for i in range(len(edges) - 1) if radius < edges[i + 1]))
    sector = int((math.atan2(point[1], point[0]) % (2. * math.pi)) / (2. * math.pi) * SECTORS) % SECTORS
    return band, sector


def straight_free(blocked, edges, goal):
    """True when the segment from the origin toward the goal, within the map, crosses no blocked cell."""
    distance = math.hypot(*goal)
    if distance <= 1e-6:
        return True
    reach = min(distance, edges[-1] - 1e-3)
    samples = int(reach) + 1
    for i in range(samples + 1):
        r = reach * i / samples
        if locate(edges, (goal[0] / distance * r, goal[1] / distance * r)) in blocked:
            return False
    return True


def plan(cells, edges, goal, hull_margin, lead=12.):
    """First waypoint toward the goal along an A* path through free cells, or None when no path exists.

    The ship starts at the origin, linked to every free cell of the two inner
    bands. The target is the goal's cell, or the free cell nearest the goal
    when the goal lies outside the map or inside blocked space. A goal with a
    free straight line gets a waypoint on that line at the lead distance, so
    the sector grid does not quantize the heading, which alternated by a
    sector from one decision to the next as the nose turned.
    """
    blocked, centers = blocked_cells(cells, edges, hull_margin)
    bands = len(edges) - 1
    free = [k for k in centers if k not in blocked]
    if not free:
        return None
    if straight_free(blocked, edges, goal):
        distance = math.hypot(*goal)
        radius = next((.5 * (edges[b] + edges[b + 1]) for b in range(bands) if .5 * (edges[b] + edges[b + 1]) >= lead), lead)
        radius = min(radius, distance)
        return (goal[0] / distance * radius, goal[1] / distance * radius) if distance > 1e-6 else None
    target = locate(edges, goal)
    if target is None or target in blocked:
        target = min(free, key=lambda k: (centers[k][0] - goal[0]) ** 2 + (centers[k][1] - goal[1]) ** 2)

    def neighbors(key):
        b, s = key
        for db in (-1, 0, 1):
            for ds in (-1, 0, 1):
                if db == 0 and ds == 0:
                    continue
                nb, ns = b + db, (s + ds) % SECTORS
                if 0 <= nb < bands and (nb, ns) not in blocked:
                    yield (nb, ns)

    def heuristic(key):
        return math.hypot(centers[key][0] - goal[0], centers[key][1] - goal[1])

    start = ('origin',)
    frontier = [(heuristic(target) if False else 0., start)]
    best = {start: 0.}; parent = {}
    found = None
    while frontier:
        _, key = heapq.heappop(frontier)
        if key == target:
            found = key
            break
        cost = best[key]
        candidates = [k for k in centers if k[0] <= 1 and k not in blocked] if key == start else list(neighbors(key))
        here = (0., 0.) if key == start else centers[key]
        for nxt in candidates:
            step = math.hypot(centers[nxt][0] - here[0], centers[nxt][1] - here[1])
            if cost + step < best.get(nxt, math.inf):
                best[nxt] = cost + step; parent[nxt] = key
                heapq.heappush(frontier, (cost + step + heuristic(nxt), nxt))
    if found is None:
        return None
    path = []
    key = found
    while key != start:
        path.append(centers[key]); key = parent[key]
    path.reverse()
    for point in path:
        if math.hypot(*point) >= lead:
            return point
    return path[-1] if path else None
