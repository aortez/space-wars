import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('calibration', Path(__file__).resolve().parents[1] / 'calibrate-transfer-references.py')
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)


def pair_fixture(root, defer=False):
    case = dict(source_tick=10, destination=1, seat=0)
    contact = dict(hull=dict(count=0), feet=[dict(count=0), dict(count=0)], landing={})
    ordinary = root / 'ordinary'
    controlled = root / 'controlled'
    for folder in [ordinary, controlled]:
        folder.mkdir()
        raw, probe = [], []
        for tick in [10, 11]:
            for seat in [0, 1]:
                interrupted = defer and folder == ordinary and tick == 11 and seat == 0
                pursuit = dict(started_tick=11, last_visible_tick=11, reason='nearby vulnerable opponent') if interrupted else None
                mission = dict(goal='hunt' if interrupted else 'capture' if tick == 11 else 'transfer',
                    target=None if interrupted else 1, recovery=None, avoidance=None, pursuit=pursuit,
                    events=[dict(kind='pursuit_started' if interrupted else 'arrived', tick=11, planet=None if interrupted else 1)] if tick == 11 else [])
                pilot = dict(tick=tick, ship={}, ship_available=True, queries_ready=True, location={'aboard': seat},
                             ship_form='ship', ship_health=100, planet=dict(index=0))
                raw.append(dict(tick=tick, seat=seat, actions=[tick], mission=mission,
                    observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot))))), landing_diagnostics=contact))
                if seat == 0:
                    comparison = None
                    if folder == controlled and tick > 10:
                        comparison = dict(deferred_new_pursuit=defer, ordinary_new_pursuit=defer,
                            same_intent=True, same_mission=not defer, ordinary_actions=[tick], ordinary_target=None,
                            ordinary_pursuit=dict(started_tick=11, last_visible_tick=11, reason='nearby vulnerable opponent'))
                    outcome = dict(tick=11, elapsed_ticks=1, reason='retargeted' if interrupted else 'arrived')
                    probe.append(dict(tick=tick, ship={}, ship_available=True, queries_ready=True, location={'aboard': 0},
                        form='ship', health=100, frame=0, goal=mission['goal'], target=mission['target'], avoidance=None,
                        next_actions=[tick], recovery_active=False, arrived=tick==11 and not interrupted, solver_contacts=contact,
                        solver_contact=False, debris_contact=False, damage=dict(last_contact_tick=None), match_finished=False,
                        terminal=outcome if tick==11 else None, pursuit_control=comparison))
        with gzip.open(folder / 'trace.jsonl.gz', 'wt') as f:
            f.writelines(json.dumps(r)+'\n' for r in raw)
        (folder / 'transfer-probe.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in probe))
        (folder / 'report.json').write_text(json.dumps(dict(transfer_probe=dict(source={}, outcome=probe[-1]['terminal'],
            pursuit_policy='ordinary' if folder==ordinary else 'defer_new'))))
    return case, ordinary, controlled


class CalibrationAudit(unittest.TestCase):
    def test_pairs_require_exact_state_before_real_suppressed_pursuit(self):
        for defer in [False, True]:
            with tempfile.TemporaryDirectory() as tmp:
                case, normal, controlled = pair_fixture(Path(tmp), defer)
                result = C.audit_pair(case, normal, controlled)
                self.assertEqual(result['first_deferred_tick'], 11 if defer else None)
                self.assertEqual(result['controlled_phases']['executed_ticks'], 1)
                rows = list(C.read_trace(controlled))
                rows[0]['actions'] = ['changed']
                with gzip.open(controlled / 'trace.jsonl.gz', 'wt') as f:
                    f.writelines(json.dumps(r)+'\n' for r in rows)
                with self.assertRaises(AssertionError):
                    C.audit_pair(case, normal, controlled)

    def test_terminal_contact_precedes_a_new_pursuit_without_executing_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            case, normal, controlled = pair_fixture(Path(tmp), defer=True)
            for root in [normal, controlled]:
                probe = C.T.rows(root / 'transfer-probe.jsonl')
                probe[-1]['terminal']['reason'] = 'solver_or_debris_contact'
                probe[-1]['solver_contact'] = True
                probe[-1]['solver_contacts']['hull']['count'] = 1
                (root / 'transfer-probe.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in probe))
                report = json.loads((root / 'report.json').read_text())
                report['transfer_probe']['outcome'] = probe[-1]['terminal']
                (root / 'report.json').write_text(json.dumps(report))
                raw = list(C.read_trace(root))
                raw[-2]['landing_diagnostics']['hull']['count'] = 1
                with gzip.open(root / 'trace.jsonl.gz', 'wt') as f:
                    f.writelines(json.dumps(r)+'\n' for r in raw)
            result = C.audit_pair(case, normal, controlled)
            self.assertEqual(result['deferred_observations'], 1)
            self.assertEqual(result['deferred_executed_observations'], 0)

    def test_no_suppression_cannot_hide_a_terminal_physical_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            case, normal, controlled = pair_fixture(Path(tmp))
            rows = C.T.rows(controlled / 'transfer-probe.jsonl')
            rows[-1]['health'] = 10
            (controlled / 'transfer-probe.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            with self.assertRaises(AssertionError):
                C.audit_pair(case, normal, controlled)

    def test_discovery_does_not_filter_unknown_or_supported_references(self):
        source = dict(seat=0, source_tick=60, current_target=0, transfer_source=dict(frame=0),
                      alternatives=[dict(destination=1, reason=None), dict(destination=2, reason='solar detour')])
        plan = C.fresh_cases([(C.discovery_plan()[0], [source])])
        self.assertEqual([c['expected_reason'] for c in plan], [None, 'solar detour'])
        bad = copy.deepcopy(source)
        bad['alternatives'][0]['destination'] = 0
        with self.assertRaises(AssertionError):
            C.fresh_cases([(C.discovery_plan()[0], [bad])])
        self.assertEqual(C.fresh_cases([(C.discovery_plan()[0], [])]), [])

    def test_phase_durations_exclude_unexecuted_terminal_control(self):
        rows = [dict(tick=7, goal='launch', frame=0, avoidance=None),
                dict(tick=8, goal='transfer', frame=0, avoidance=None),
                dict(tick=9, goal='hunt', frame=1, avoidance=None)]
        result = C.phases(rows)
        self.assertEqual(result['goals'], dict(launch=1, transfer=1))
        self.assertEqual(result['executed_ticks'], 2)


if __name__ == '__main__':
    unittest.main()
