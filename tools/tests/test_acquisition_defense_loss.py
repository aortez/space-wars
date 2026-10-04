import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('loss', Path(__file__).parents[1] / 'diagnose-acquisition-defense-loss.py')
L = importlib.util.module_from_spec(spec)
spec.loader.exec_module(L)


def prior_fixture():
    runs = {}
    for arm, enabled in (('off', False), ('on', True)):
        name = L.GROUP + '-' + arm
        item = dict(group=L.GROUP, seed=14699744800433948105, seat=1, opponent=10,
                    interval=3, arm='candidate', laser=True, defense=enabled)
        runs[name] = dict(item=item, command=['binary', L.A.FLAG, '1' if enabled else 'none',
            L.A.L.FLAG, '1', '--trace-start-tick', '0', '--trace-end-tick', '36001', '--out', arm])
    return dict(complete=True, profile=L.A.PROFILE, binary=dict(sha256=L.BINARY_SHA256), runs=runs)


def points(requested=(True, False), fired=(1, 0)):
    return {s: [dict(tick=t, action=dict(cannon=bool(t == 2 and requested[s])),
                     weapons=dict(shells_fired=fired[s] if t >= 3 else 0)) for t in range(13)] for s in (0, 1)}


def contact():
    return dict(tick=12, damage=dict(debris_contacts=1, last_contact_tick=12,
                                     last_contact_source='cannon', last_contact_spawn_tick=2))


class AcquisitionDefenseLossTests(unittest.TestCase):
    def test_selection_keeps_both_arms_of_the_same_recorded_counterexample(self):
        prior = prior_fixture()
        selected = L.select(prior)
        self.assertEqual([r['item']['defense'] for r in selected], [False, True])
        for key, value in (('complete', False), ('profile', 'another')):
            with self.assertRaises(AssertionError): L.select(dict(prior, **{key: value}))
        prior['binary']['sha256'] = 'different'
        with self.assertRaises(AssertionError): L.select(prior)

    def test_wrong_seat_opponent_world_or_option_is_not_the_selected_case(self):
        for key, value in (('seat', 0), ('opponent', 16), ('seed', 42), ('interval', 0),
                           ('arm', 'control'), ('laser', False), ('defense', False)):
            prior = prior_fixture()
            prior['runs'][L.GROUP + '-on']['item'][key] = value
            with self.subTest(key=key), self.assertRaises(AssertionError): L.select(prior)
        prior = prior_fixture()
        prior['runs'][L.GROUP + '-on']['command'][2] = '0'
        with self.assertRaises(AssertionError): L.select(prior)

    def test_native_observer_preserves_each_original_defense_and_laser_flag(self):
        for run in L.select(prior_fixture()):
            before = copy.deepcopy(run)
            command = L.N.command(run, Path('diagnostic'))
            flags = dict(zip(command[1::2], command[2::2]))
            self.assertEqual(flags[L.A.FLAG], '1' if run['item']['defense'] else 'none')
            self.assertEqual(flags[L.A.L.FLAG], '1')
            self.assertEqual(flags['--impact-pod-control'], 'bot')
            self.assertEqual(flags['--out'], 'diagnostic')
            self.assertEqual(run, before)

    def test_unchanged_contact_counter_cannot_reuse_a_stale_stamp(self):
        row = contact()
        row['damage']['last_contact_tick'] = 10
        self.assertIsNone(L.contact_receipt(row, 1, points()))
        with self.assertRaises(AssertionError): L.contact_receipt(row, 0, points())
        with self.assertRaises(AssertionError): L.contact_receipt(row, 2, points())

    def test_projectile_age_requires_a_past_native_spawn_clock(self):
        row = contact()
        result = L.contact_receipt(row, 0, points())
        self.assertEqual(result['age_ticks'], 10)
        for spawn in (None, -1, 12, 13):
            row['damage']['last_contact_spawn_tick'] = spawn
            with self.subTest(spawn=spawn), self.assertRaises(AssertionError): L.contact_receipt(row, 0, points())

    def test_requests_are_not_proof_of_an_actual_shot(self):
        result = L.contact_receipt(contact(), 0, points(requested=(True, True), fired=(1, 0)))
        self.assertEqual(result['firing'], [dict(seat=0, requested=True, fired_delta=1)])
        result = L.contact_receipt(contact(), 0, points(fired=(0, 0)))
        self.assertEqual(result['firing'], [])
        with self.assertRaises(AssertionError):
            L.contact_receipt(contact(), 0, points(requested=(False, False)))

    def test_ambiguous_shooters_are_retained_without_fabricated_identity(self):
        result = L.contact_receipt(contact(), 0, points(requested=(True, True), fired=(1, 1)))
        self.assertEqual([f['seat'] for f in result['firing']], [0, 1])
        self.assertNotIn('projectile_id', result)
        self.assertNotIn('shooter', result)

    def test_multiple_contacts_and_non_cannon_provenance_remain_explicit(self):
        row = contact()
        row['damage'].update(debris_contacts=3, last_contact_source='fragment')
        result = L.contact_receipt(row, 1, points())
        self.assertEqual(result['contacts_this_step'], 2)
        self.assertEqual(result['source'], 'fragment')
        self.assertNotIn('firing', result)


if __name__ == '__main__':
    unittest.main()
