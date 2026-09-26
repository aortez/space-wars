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
    case = dict(source_tick=100, destination=1, expected_reason=reason)
    outcome = dict(tick=101, elapsed_ticks=1, reason='arrived')
    probe = dict(source_tick=100, destination=1, outcome=outcome, source=dict(tick=100,
        nomination=dict(accepted=True, reason=None),
        diagnostic=dict(destination=1, reason=reason, reference=None, completed_stages={},
                        geometry=dict(separation=10, threshold=30, body=0, check='moving_planet', leg='transfer'))))
    motion = dict(position=dict(x=0, y=60), velocity=dict(x=0, y=0))
    row = dict(tick=100, target=1, frame=0, terminal=None, queries_ready=True, arrived=False,
               solver_contact=False, debris_contact=False, avoidance=None, health=100, ship=motion,
               planets=[dict(index=1, radius=50, motion=dict(position=dict(x=0, y=0), velocity=dict(x=0, y=0)))])
    trace = [row, dict(row, tick=101, frame=1, arrived=True, terminal=outcome)]
    return case, probe, trace


class TransferReferenceAudit(unittest.TestCase):
    def test_arrival_requires_real_event_queries_motion_and_no_contact(self):
        case, probe, trace = fixture()
        self.assertTrue(T.audit_probe(case, probe, trace)['accepted'])
        for mutation in ['missing_event', 'queries', 'frame', 'contact', 'speed', 'range', 'missing_tick', 'wrong_reason', 'unsupported_cost']:
            c, p, t = copy.deepcopy((case, probe, trace))
            if mutation == 'missing_event': t[-1]['arrived'] = False
            if mutation == 'queries': t[-1]['queries_ready'] = False
            if mutation == 'frame': t[-1]['frame'] = 0
            if mutation == 'contact': t[-1]['solver_contact'] = True
            if mutation == 'speed': t[-1]['ship']['velocity']['x'] = 18
            if mutation == 'range': t[-1]['ship']['position']['y'] = 155
            if mutation == 'missing_tick': t.pop(0)
            if mutation == 'wrong_reason': p['source']['diagnostic']['reason'] = 'other'
            if mutation == 'unsupported_cost': p['source']['diagnostic']['reference'] = {}
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                T.audit_probe(c, p, t)

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
