"""LF-066 synthetic checks for force balance and saved-update interpretations."""
import unittest
import torch
from tools.flight_motor import sustained
from . import motor_balance as audit
from . import motor_feedback as feedback
from .motor_feedback_tests import inputs


class BalanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def test_crossings_keep_stability_direction_and_brackets(self):
        r = audit.crossings([0, 1, 2, 3], [2, -2, -1, 3])
        self.assertEqual([v['speed'] for v in r], [.5, 2.25])
        self.assertEqual([v['restoring'] for v in r], [True, False])
        self.assertEqual(r[0]['bracket'], [0, 1])
        self.assertEqual(r[0]['slope'], -4)

    def test_no_crossing_and_exact_zeros_do_not_invent_stability(self):
        self.assertEqual(audit.crossings([0, 1, 2], [1, 2, 3]), [])
        r = audit.crossings([0, 1, 2], [0, 0, 0])
        self.assertEqual([v['speed'] for v in r], [0, 1, 2])
        self.assertTrue(all(v['restoring'] is None for v in r))

    def test_dense_grid_has_declared_order_and_consistent_velocity(self):
        anchors = [{'inputs': x.tolist()} for x in inputs()]
        x = audit.dense_grid(anchors)
        self.assertEqual(len(x), 3 * 6 * 61)
        q = feedback.direction(x)
        torch.testing.assert_close((x[:, 3:5] * 20 * q).sum(1), torch.tensor(audit.SPEEDS * 18), atol=5e-6, rtol=1e-6)
        self.assertTrue(torch.equal(x[:61, 2], torch.full((61,), .2)))

    def test_near_target_groups_preserve_command_and_readiness(self):
        x = inputs().repeat(2, 1); x[:, 0] = 0; x[:, 1] = 1; x[:, 3:8] = 0
        x = feedback.alter(x, [4, 4, 10, 10, 16, 16], [2, 5, 9, 12, 16, 20])
        _, g = audit.masks(x)
        self.assertEqual(g['near_under_command_4'].tolist(), [True, False, False, False, False, False])
        self.assertEqual(int(g['near_over_command_10'].sum()), 1)
        self.assertEqual(int(g['near_over_command_16'].sum()), 0)
        x[3, 7] = 1
        self.assertEqual(int(audit.masks(x)[1]['near_over_command_10'].sum()), 0)

    def test_local_grid_keeps_fractional_error_metadata(self):
        x, metadata = audit.local_grid([{'inputs': row.tolist()} for row in inputs()])
        torch.testing.assert_close(metadata[:5, 2], torch.tensor([-1., -.25, 0., .25, 1.]))
        forward = (x[:, 3:5] * 20 * feedback.direction(x)).sum(1)
        torch.testing.assert_close(forward - metadata[:, 1], metadata[:, 2], atol=3e-6, rtol=1e-5)

    def test_passive_braking_slope_separates_from_constant_actions(self):
        x = torch.zeros(54, 18); x[:, 1] = 1; x[:, 10:15] = 1
        local, _ = feedback.grid([{'inputs': r.tolist()} for r in x], [0, *audit.COMMANDS], audit.LOCAL_ERRORS)
        actions = torch.full((len(local), 2), .2)
        values = {'actions': actions, 'acceleration': feedback.acceleration(local, actions)}
        result = audit.local_summary(local, values)
        self.assertTrue(torch.equal(result['acceleration_slope'], result['passive_slope']))
        self.assertEqual(int(result['action_slope'].count_nonzero()), 0)
        self.assertAlmostEqual(float(result['acceleration_slope'][0, 1]), -.8, places=5)
        self.assertEqual(float(result['acceleration_slope'][0, -1]), 0.)

    def test_saved_training_summary_keeps_zero_command_unchanged(self):
        torch.manual_seed(66)
        before = sustained.ResidualModel({'version': 1, 'features': list(range(18))})
        after = sustained.ResidualModel(before.schema); after.load_state_dict(before.state_dict())
        x = torch.zeros(8, 18); x[:, 1] = 1; x[:, 10:15] = 1; x[:4, 2] = .5
        x[:4, 4] = torch.tensor([.4, .45, .55, .6]); x[:, 6] = x[:, 4]
        with torch.no_grad():
            raw, _ = before(x); z = raw[:, 1:] + torch.tensor([.2, -.1])
            logp = before.log_prob(raw, raw[:, 0], z, x)
            after.residual.bias.add_(torch.tensor([.1, -.1]))
        rollout = {'observations': x.reshape(2, 4, 18), 'latent': raw[:, 0].reshape(2, 4),
                   'actuator_latents': z.reshape(2, 4, 2), 'logp': logp.reshape(2, 4),
                   'advantages': torch.arange(8.).reshape(2, 4)}
        stats, error = audit.training_summary(rollout, before, after)
        self.assertEqual(error, 0.)
        self.assertEqual(stats['stopping']['delta_acceleration'], 0.)
        self.assertGreater(stats['near_under_command_10']['corrective_acceleration_delta'], 0.)
        self.assertLess(stats['near_over_command_10']['corrective_acceleration_delta'], 0.)
        self.assertLess(stats['near_over_command_10']['corrective_brake_score'], 0.)

    def test_window_aggregation_weights_rows_not_updates(self):
        records = [{'update': 1, 'groups': {'g': {'count': 1, 'x': 2.}}},
                   {'update': 2, 'groups': {'g': {'count': 3, 'x': 12.}}}]
        self.assertEqual(audit.aggregate(records, 1, 2)['g'], {'count': 4, 'x': 3.5})
        self.assertEqual(audit.aggregate(records, 2, 2)['g'], {'count': 3, 'x': 4.})


if __name__ == '__main__':
    unittest.main(verbosity=2)
