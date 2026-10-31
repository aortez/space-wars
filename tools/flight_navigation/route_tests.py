"""Polar-map A* routing: cell geometry, inflation, and detours around a sensed wall."""
import math
import unittest
from . import route

EDGES = [0., 2., 4., 6., 8., 11., 14., 18., 23., 29., 36., 44., 54., 66., 80., 96., 114., 134., 156., 180., 206., 234., 266., 300.]


def empty_cells():
    return [[0., 0.] for _ in range((len(EDGES) - 1) * route.SECTORS)]


def wall_cells(distance, half_width):
    """Block every cell whose center lies on a wall segment ahead (+y) at the given distance."""
    cells = [[0., 0.] for _ in range((len(EDGES) - 1) * route.SECTORS)]
    for b in range(len(EDGES) - 1):
        for s in range(route.SECTORS):
            x, y = route.cell_center(EDGES, b, s)
            if abs(y - distance) < 3. and abs(x) < half_width:
                cells[b * route.SECTORS + s] = [1., 1.]
    return cells


class RouteTests(unittest.TestCase):
    def test_cell_centers_and_location_agree(self):
        for b in [0, 5, 12, 22]:
            for s in [0, 16, 33, 63]:
                self.assertEqual(route.locate(EDGES, route.cell_center(EDGES, b, s)), (b, s))
        self.assertIsNone(route.locate(EDGES, (400., 0.)))

    def test_straight_waypoint_on_an_empty_map(self):
        point = route.plan(empty_cells(), EDGES, (0., 100.), hull_margin=9.)
        self.assertIsNotNone(point); self.assertGreater(point[1], 10.); self.assertLess(abs(point[0]), 6.)

    def test_a_free_straight_line_keeps_the_exact_goal_heading(self):
        # The waypoint lies on the goal's own direction at the lead radius, not on a sector center.
        for goal in [(30., 100.), (-250., 40.), (5., -90.)]:
            point = route.plan(empty_cells(), EDGES, goal, hull_margin=9.)
            self.assertAlmostEqual(math.atan2(point[1], point[0]), math.atan2(goal[1], goal[0]), places=6)
            self.assertGreaterEqual(math.hypot(*point), 12.)
        # A goal inside the lead radius is the waypoint itself.
        self.assertEqual(route.plan(empty_cells(), EDGES, (3., 4.), hull_margin=9.), (3., 4.))
        # A wall across the line still routes around it on the grid.
        self.assertFalse(route.straight_free(route.blocked_cells(wall_cells(60., 30.), EDGES, 9.)[0], EDGES, (0., 120.)))

    def test_route_goes_around_a_wall(self):
        cells = wall_cells(60., 30.)
        blocked, _ = route.blocked_cells(cells, EDGES, 9.)
        self.assertIn(route.locate(EDGES, (0., 60.)), blocked)
        point = route.plan(cells, EDGES, (0., 120.), hull_margin=9.)
        self.assertIsNotNone(point)
        self.assertGreater(abs(math.atan2(point[0], point[1])), math.radians(10.))  # The first waypoint already bends away from straight ahead.

    def test_no_route_through_a_closed_ring(self):
        cells = [[0., 0.] for _ in range((len(EDGES) - 1) * route.SECTORS)]
        for s in range(route.SECTORS):
            cells[8 * route.SECTORS + s] = [1., 1.]
        self.assertIsNone(route.plan(cells, EDGES, (0., 120.), hull_margin=1.))


if __name__ == '__main__':
    unittest.main()
