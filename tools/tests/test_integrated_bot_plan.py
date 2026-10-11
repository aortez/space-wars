import copy
from collections import Counter, defaultdict
import importlib.util
from pathlib import Path
import unittest


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TOOLS = Path(__file__).parents[1]
M = load('integrated_bot_plan', TOOLS / 'plan-integrated-bot.py')


class IntegratedBotPlanTests(unittest.TestCase):
    def test_complete_pairs_and_balanced_fresh_strata(self):
        cases = M.plan()
        self.assertEqual(Counter(c['stage'] for c in cases), dict(qualification=16, held_out=96))
        self.assertEqual(len({c['name'] for c in cases}), len(cases))
        groups = defaultdict(list)
        for case in cases:
            groups[case['group']].append(case)
        for pair in groups.values():
            self.assertEqual({c['arm'] for c in pair}, {'candidate', 'control'})
            self.assertEqual([{k: v for k, v in c.items() if k not in ('arm', 'name')}
                              for c in pair][0],
                             [{k: v for k, v in c.items() if k not in ('arm', 'name')}
                              for c in pair][1])
        fresh = [c for c in cases if c['stage'] == 'held_out']
        self.assertEqual(len({c['seed'] for c in fresh}), 4)
        strata = Counter((c['opponent'], c['seat'], c['interval'], c['arm']) for c in fresh)
        self.assertEqual(len(strata), 24)
        self.assertEqual(set(strata.values()), {4})
        self.assertTrue(all(c['seed'] == M.seed(c['seed_namespace']) for c in fresh))
        for c in fresh:
            flags = M.flags(c)
            self.assertEqual((flags['--mode'], flags['--seconds'], flags['--require-finish']),
                             ('duel', '600', 'true'))
            self.assertEqual(flags['--seat'], '0')

    def test_candidate_does_not_reconfigure_opponent_or_shared_host(self):
        for before, after in zip(M.plan()[::2], M.plan()[1::2]):
            a, b = M.flags(before), M.flags(after)
            differences = {key for key in a if a[key] != b[key]}
            self.assertEqual(differences, {'--' + key for key in M.SEAT_OPTIONS}
                             | {'--active-flight-checks'})
            for case, flags in ((before, a), (after, b)):
                seat = case['seat']
                self.assertEqual(flags[f'--p{seat+1}-policy'], 'material_mission_v13')
                self.assertEqual(flags[f'--p{2-seat}-policy'], f"material_mission_v{case['opponent']}")
                for name in M.SEAT_OPTIONS:
                    self.assertEqual(flags['--' + name], str(seat) if case['arm'] == 'candidate' else 'none')
                for name in M.DISABLED_SEAT_OPTIONS:
                    self.assertEqual(flags['--' + name], 'none')
                self.assertEqual(flags['--probe-projectile-response'], 'none')

    def test_route_stack_matches_retained_flight_configuration(self):
        old = load('retained_flight_plan', TOOLS / 'validate-flight-continuation.py')
        for seat in (0, 1):
            source = next(c for c in old.P.plan() if c['name'] == f'shared-armed-world1-p{seat+1}-powered')
            args = old.arguments(source, True)
            prior = dict(zip(args[::2], args[1::2]))
            case = next(c for c in M.plan() if c['name'] == f'known-powered-world1-p{seat+1}-candidate')
            current = M.flags(case)
            # The native cover response replaces the regressed optional search.
            # Extra report/disabled flags do not silently change retained inputs.
            self.assertEqual({k for k in prior if prior[k] != current[k]}, {'--cover-response-seats'})

    def test_smoke_does_not_consume_evaluation_seeds_or_claim_finished_matches(self):
        smoke = M.smoke_plan()
        self.assertEqual(len(smoke), 12)
        self.assertEqual({c['seed'] for c in smoke}, {42})
        self.assertFalse({c['seed'] for c in smoke} &
                         {c['seed'] for c in M.plan() if c['stage'] == 'held_out'})
        for case in smoke:
            self.assertEqual(M.flags(case)['--seconds'], '1')
            self.assertEqual(M.flags(case)['--require-finish'], 'false')

    def test_rejects_unplanned_seats_arms_and_opponents(self):
        for key, value in (('seat', 2), ('arm', 'latest'), ('opponent', 17)):
            case = dict(M.plan()[0], **{key: value})
            with self.assertRaises(ValueError):
                M.flags(case)

    def report(self, case):
        candidate, seat = case['arm'] == 'candidate', case['seat']
        enabled = [candidate and s == seat for s in (0, 1)]
        descriptors = [dict(policy=f"material_mission_v{case['opponent']}",
                            sensor_profile='mission_cadenced_joint_routes_v1') for _ in (0, 1)]
        descriptors[seat]['policy'] = 'material_mission_v13'
        report = dict(physics_ok=True, audit_failures=[], policy_configuration=descriptors,
                      mission_evaluation={}, seed=42, mode='duel', seat=0, elapsed_ticks=60,
                      combat_breaks=dict(interval_seconds=15, duration_seconds=4), landing_survey_hz=4,
                      pursuit_disengagement=dict(enabled_seats=[False, False], probe_handoff=False,
                                                 boundary_guidance=False, destination_cover_probe=False),
                      live_objective_planning=dict(enabled_seats=[0, 1],
                          allowance=dict(graph=4, physics_queries=384), objective_dependencies='routes',
                          **{k.replace('-', '_'): True for k in M.ROUTE_OPTIONS}))
        if candidate:
            descriptors[seat]['sensor_profile'] = 'mission_cadenced_jetpack_routes_capture_value_v1'
            descriptors[seat].update(M.MODELS)
            for key, profile in M.PROFILES.items():
                report[key] = dict(enabled_seats=enabled, profile=profile)
            for key in ('flag_cost_admission', 'current_neutral_survey'):
                report['mission_evaluation'][key] = dict(enabled_seats=enabled)
        return report

    def test_report_must_confirm_requested_seat_and_options(self):
        for case in M.smoke_plan():
            M.check_configuration(self.report(case), case)
        case = next(c for c in M.smoke_plan() if c['seat'] == 1 and c['arm'] == 'candidate')
        report = self.report(case)
        for mutate in (
            lambda r: r['powered_capture'].update(enabled_seats=[True, False]),
            lambda r: r['active_flight_checks'].update(profile='unreviewed_model'),
            lambda r: r['policy_configuration'][1].update(current_neutral_model='unreviewed_model'),
            lambda r: r['mission_evaluation']['current_neutral_survey'].update(enabled_seats=[True, True]),
            lambda r: r['live_objective_planning']['allowance'].update(physics_queries=385),
            lambda r: r['live_objective_planning'].update(powered_objective_routes=False),
            lambda r: r.update(cover_response=dict(enabled_seats=[False, True])),
            lambda r: r.update(projectile_response={}),
            lambda r: r['policy_configuration'][0].update(policy='material_mission_v13'),
            lambda r: r.update(seed=43),
            lambda r: r.update(elapsed_ticks=36000),
            lambda r: r['combat_breaks'].update(interval_seconds=0),
            lambda r: r.update(physics_ok=False),
        ):
            changed = copy.deepcopy(report)
            mutate(changed)
            with self.assertRaises(ValueError):
                M.check_configuration(changed, case)


if __name__ == '__main__':
    unittest.main()
