import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('transfer', Path(__file__).resolve().parents[1] / 'probe-transfer-references.py')
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)


def fixture():
    reason = 'moving body requires an unmodelled transfer detour'
    source = dict(frame=0, ship=dict(position=dict(x=0., y=120.), velocity=dict(x=0., y=20.)), bodies=[
        dict(index=0, position=dict(x=0., y=0.), velocity=dict(x=0., y=20.), radius=50.),
        dict(index=1, position=dict(x=350., y=200.), velocity=dict(x=0., y=0.), radius=50.)])
    length = (350**2 + 80**2)**0.5
    case = dict(source_tick=100, destination=1, expected_reason=reason, transfer_source=source)
    outcome = dict(tick=101, elapsed_ticks=1, reason='arrived')
    probe = dict(source_tick=100, destination=1, outcome=outcome, source=dict(tick=100,
        nomination=dict(accepted=True, reason=None),
        diagnostic=dict(destination=1, reason=reason, reference=None, completed_stages=dict(settle_seconds=0, turn_seconds=0, climb_seconds=0, cruise_seconds=10),
                        geometry=dict(separation=0, threshold=115, body=0, check='moving_planet', leg='transfer',
                            obstacle_from=dict(x=0, y=0), obstacle_to=dict(x=0, y=200),
                            **{'from': dict(x=0, y=120), 'to': dict(x=350-350/length*135, y=200-80/length*135)}))))
    motion = dict(position=dict(x=350, y=260), velocity=dict(x=0, y=0))
    row = dict(tick=100, target=1, frame=0, terminal=None, queries_ready=True, arrived=False,
               solver_contact=False, debris_contact=False, avoidance=None, health=100, ship=motion,
               ship_available=True, form='ship', location='aboard', recovery_active=False, match_finished=False,
               planets=[dict(index=1, radius=50, motion=dict(position=dict(x=350, y=200), velocity=dict(x=0, y=0)))])
    trace = [row, dict(row, tick=101, frame=1, arrived=True, terminal=outcome)]
    return case, probe, trace


class TransferReferenceAudit(unittest.TestCase):
    def test_arrival_requires_real_event_queries_motion_and_no_contact(self):
        case, probe, trace = fixture()
        self.assertTrue(T.audit_probe(case, probe, trace)['accepted'])
        for mutation in ['missing_event', 'queries', 'frame', 'contact', 'speed', 'range', 'missing_tick', 'wrong_reason', 'unsupported_cost', 'unavailable', 'recovery', 'dead', 'pod', 'on_foot', 'blocker', 'margin', 'separation', 'sweep']:
            c, p, t = copy.deepcopy((case, probe, trace))
            if mutation == 'missing_event': t[-1]['arrived'] = False
            if mutation == 'queries': t[-1]['queries_ready'] = False
            if mutation == 'frame': t[-1]['frame'] = 0
            if mutation == 'contact': t[-1]['solver_contact'] = True
            if mutation == 'speed': t[-1]['ship']['velocity']['x'] = 18
            if mutation == 'range': t[-1]['ship']['position']['y'] = 355
            if mutation == 'missing_tick': t.pop(0)
            if mutation == 'wrong_reason': p['source']['diagnostic']['reason'] = 'other'
            if mutation == 'unsupported_cost': p['source']['diagnostic']['reference'] = {}
            if mutation == 'unavailable': t[-1]['ship_available'] = False
            if mutation == 'recovery': t[-1]['recovery_active'] = True
            if mutation == 'dead': t[-1]['health'] = 0
            if mutation == 'pod': t[-1]['form'] = 'escape_pod'
            if mutation == 'on_foot': t[-1]['location'] = 'on_foot'
            if mutation == 'blocker': p['source']['diagnostic']['geometry']['body'] = 999
            if mutation == 'margin': p['source']['diagnostic']['geometry']['threshold'] = 1000
            if mutation == 'separation': p['source']['diagnostic']['geometry']['separation'] = 1
            if mutation == 'sweep': p['source']['diagnostic']['geometry']['obstacle_to']['y'] = 100
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, KeyError)):
                T.audit_probe(c, p, t)

    def test_contact_takes_precedence_over_simultaneous_arrival(self):
        case, probe, trace = fixture()
        trace[-1]['solver_contact'] = True
        probe['outcome']['reason'] = 'solver_or_debris_contact'
        self.assertEqual(T.audit_probe(case, probe, trace)['outcome']['reason'], 'solver_or_debris_contact')

    def test_f32_identity_is_bit_exact_across_serializers(self):
        self.assertEqual(T.f32_identity(1.8), T.f32_identity(1.7999999523162842))
        self.assertNotEqual(T.f32_identity(1.8), T.f32_identity(1.8000001))

    def test_refusal_is_retained_and_never_timed_as_a_flight(self):
        case, probe, trace = fixture()
        probe['source']['nomination'] = dict(accepted=False, reason='capture committed')
        probe['outcome'] = dict(tick=100, elapsed_ticks=0, reason='nomination_refused')
        trace = [dict(trace[0], terminal=probe['outcome'])]
        result = T.audit_probe(case, probe, trace)
        self.assertFalse(result['accepted'])
        probe['outcome']['elapsed_ticks'] = 60
        with self.assertRaises(AssertionError):
            T.audit_probe(case, probe, trace)

    def test_prefix_requires_both_source_observations_and_all_prior_controls(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            lines = [dict(tick=tick, seat=seat, observation={'ship': tick}, controls={'x': 1})
                     for tick in range(2) for seat in range(2)]
            encoded = [(json.dumps(r) + '\n').encode() for r in lines]
            (root / 'trace.jsonl').write_bytes(b''.join(encoded))
            (root / 'mission-evaluations.jsonl').write_text('')
            case = dict(source_tick=1, seat=0, transfer_source={'frame': 1})
            probe = dict(source=dict(transfer_source={'frame': 1}, nomination=dict(accepted=False)))
            expected = dict(prefix_rows=2, prefix_sha256=T.hashlib.sha256(b''.join(encoded[:2])).hexdigest(),
                            source_rows={0: lines[2], 1: lines[3]}, evaluations=[])
            T.audit_prefix(root, case, expected, probe)
            for index in [0, 2, 3]:
                bad = copy.deepcopy(lines)
                bad[index]['controls']['x'] = -1
                (root / 'trace.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in bad))
                with self.subTest(index=index), self.assertRaises(AssertionError):
                    T.audit_prefix(root, case, expected, probe)


if __name__ == '__main__':
    unittest.main()
