import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('comparison', Path(__file__).resolve().parents[1] / 'compare-transfer-destinations.py')
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)
FIXTURE_SPEC = importlib.util.spec_from_file_location('forecast_fixture', Path(__file__).with_name('test_transfer_forecast.py'))
F = importlib.util.module_from_spec(FIXTURE_SPEC)
FIXTURE_SPEC.loader.exec_module(F)


def fixture():
    def forecast(ticks, end=None):
        return dict(source_tick=10, destination=2, horizon_ticks=3600, ticks=ticks,
            charged_graph=ticks, samples=[], end=end, handoff_seconds=.033333335 if end else None)
    def report(ticks, ranked=False):
        return dict(model='guided_transfer_comparison_v1', actor='player_1', source_tick=10,
            current_destination=1, current_selected_tick=5, current_goal='capture',
            pre_intent_destination=1, evaluator_source_tick=4, candidates_truncated=False,
            charged_graph=ticks+int(ranked), ranked=ranked, preferred_handoffs=[],
            fastest_known_handoffs=[2] if ranked else [], candidates=[
                dict(destination=1, current=True, nomination=None, source_actions=['actual'],
                     unknown='local phase unsupported', forecast=None),
                dict(destination=2, current=False, nomination=dict(destination=2, accepted=True, reason=None),
                     source_actions=['hypothetical'], unknown=None,
                     forecast=forecast(ticks, 'kinematic_handoff' if ranked else None))])
    token = dict(actor=0, generation=1)
    pending = dict(token=token, actor='player_1', source_tick=10, destination=1,
        validated_tick=10, completed_tick=None, cancelled_tick=None, phase='pending', reason=None, charged_graph=1)
    ready = dict(pending, validated_tick=11, completed_tick=11, phase='ready', charged_graph=3)
    stale = dict(ready, validated_tick=None, cancelled_tick=12, phase='stale', reason='physical contact')
    final = report(2, True)
    source = dict(seat=0, source_tick=10, attempted=True, rejected=None, initial=report(0),
        last_snapshot=final, last_snapshot_tick=11, published=final, published_state=ready, final_state=stale)
    schedule = dict(observational=True, physics_queries=0, playing_graph_allowance=4,
        max_source_age_ticks=120, total_graph_allowance=32, sources=[source],
        charged_graph=3, submitted=1, completed=1, cancelled=1)
    work = lambda graph: dict(graph=graph, physics_queries=0)
    rows = []
    for i, (s, charged, r) in enumerate([(pending, 1, report(1)), (ready, 2, final), (stale, 0, final)]):
        candidates = [dict(destination=c['destination'], unknown=c['unknown'],
            charged_graph=c['forecast']['charged_graph'] if c['forecast'] else 0,
            end=c['forecast']['end'] if c['forecast'] else None) for c in r['candidates']]
        rows.append(dict(event='dispatch', tick=10+i, playing_charged_graph=4,
            remaining_before_comparison=work(28), allocation=dict(tick=i+1, allowance=work(28),
                charged=work(charged), jobs=[] if i == 2 else [dict(request=token, charged=work(charged), phase=s['phase'].title())]),
            actors=[dict(seat=0, state=s, rejected=None, snapshot_tick=min(10+i, 11), ranked=r['ranked'], candidates=candidates)]))
    return schedule, rows, {tick: dict(total=4) for tick in range(10, 13)}, 13


class ComparisonAudit(unittest.TestCase):
    def test_new_candidates_are_checked_against_geometry_without_an_old_prediction(self):
        _, probe, control, _ = F.fixture()
        forecast = probe['forecast']
        C.audit_candidate(forecast['report'], control['observation'], forecast['environment'])
        for change in ['range', 'speed', 'target', 'frame', 'sample', 'source', 'nonfinite']:
            f = copy.deepcopy(forecast)
            sample = f['report']['samples'][-1]
            if change == 'range': sample['ship']['position']['x'] = 10000
            elif change == 'speed': sample['ship']['velocity']['x'] = 10000
            elif change == 'target': sample['target']['position']['x'] += 1
            elif change == 'frame': sample['frame'] = 0
            elif change == 'sample': f['report']['samples'].pop(1)
            elif change == 'source': f['report']['samples'][0]['gravity']['x'] = 100
            else: sample['ship']['spin'] = float('nan')
            with self.subTest(change=change), self.assertRaises(AssertionError):
                C.audit_candidate(f['report'], control['observation'], f['environment'])

    def test_source_shortlist_and_command_are_derived_from_ordinary_play(self):
        _, probe, control, _ = F.fixture()
        control['mission'] = dict(target=1, goal='capture', events=[dict(kind='selected', planet=1, tick=5)])
        for p in control['observation']['planets']: p['claim'] = None
        normal = [dict(copy.deepcopy(control), tick=9), control]
        schedule = fixture()[0]
        source = schedule['sources'][0]
        source['environment'] = probe['forecast']['environment']
        r = source['initial']
        r['candidates'][0]['source_actions'] = control['actions']
        r['candidates'][1].update(destination=0, forecast=None, unknown='unsupported')
        source['last_snapshot'] = copy.deepcopy(r)
        case = dict(sources=[dict(seat=0, tick=10)])
        C.audit_sources(case, schedule, normal)
        for change in ['omit', 'truncate', 'selection', 'goal', 'command', 'actor', 'source', 'unplanned']:
            s = copy.deepcopy(schedule)
            r = s['sources'][0]['initial']
            if change == 'omit': r['candidates'].pop()
            elif change == 'truncate': r['candidates_truncated'] = True
            elif change == 'selection': r['current_selected_tick'] = 9
            elif change == 'goal': r['current_goal'] = 'launch'
            elif change == 'command': r['candidates'][0]['source_actions'] = ['hypothetical']
            elif change == 'actor': r['actor'] = 'player_2'
            elif change == 'source': r['source_tick'] += 1
            else: s['sources'] *= 2
            with self.subTest(change=change), self.assertRaises(AssertionError): C.audit_sources(case, s, normal)

    def test_unknown_current_with_known_alternative_is_not_a_preference(self):
        schedule, *rest = fixture()
        result = C.audit_schedule(schedule, *rest)
        self.assertTrue(result[0]['completed'])
        self.assertEqual(result[0]['fastest_known_handoffs'], [2])
        self.assertEqual(result[0]['preferred_handoffs'], [])

    def test_ranking_checks_ties_completion_unknowns_and_its_own_charge(self):
        original = fixture()[0]['sources'][0]['last_snapshot']
        for change in ['preference', 'charge', 'time', 'partial', 'source', 'refused', 'negative', 'nan']:
            r = copy.deepcopy(original)
            if change == 'preference': r['preferred_handoffs'] = [2]
            elif change == 'charge': r['charged_graph'] -= 1
            elif change == 'time': r['candidates'][1]['forecast']['ticks'] += 1
            elif change == 'partial': r['candidates'][1]['forecast']['end'] = None
            elif change == 'source': r['source_tick'] += 1
            elif change == 'refused': r['candidates'][1]['nomination']['accepted'] = False
            else: r['candidates'][1]['forecast']['handoff_seconds'] = -5 if change == 'negative' else float('nan')
            with self.subTest(change=change), self.assertRaises(AssertionError): C.audit_report(r)

    def test_work_and_source_lifetime_cannot_be_fabricated(self):
        for change in ['duplicate', 'overspend', 'query', 'token', 'stale_work', 'snapshot', 'late', 'anchor', 'command', 'publish', 'completion_lost', 'candidate_labeled']:
            s, rows, upstream, elapsed = copy.deepcopy(fixture())
            if change == 'duplicate': rows[1]['tick'] = 10
            elif change == 'overspend': rows[0]['allocation']['charged']['graph'] = 30
            elif change == 'query': rows[0]['allocation']['charged']['physics_queries'] = 1
            elif change == 'token': rows[0]['allocation']['jobs'][0]['request'] = dict(actor=1, generation=1)
            elif change == 'stale_work': rows[2]['allocation']['jobs'] = [dict(request=rows[0]['actors'][0]['state']['token'], charged=dict(graph=0, physics_queries=0), phase='Ready')]
            elif change == 'snapshot': rows[2]['actors'][0]['snapshot_tick'] = 12
            elif change == 'late': rows[1]['actors'][0]['state']['completed_tick'] = 9
            elif change == 'anchor': rows[0]['actors'][0]['state']['destination'] = 2
            elif change == 'command': s['sources'][0]['initial']['candidates'][0]['source_actions'] = ['hypothetical']
            elif change == 'publish': s['sources'][0]['published'] = None
            elif change == 'completion_lost': rows[2]['actors'][0]['state']['completed_tick'] = None
            else: rows[0]['actors'][0]['candidates'][0]['destination'] = 2
            with self.subTest(change=change), self.assertRaises(AssertionError): C.audit_schedule(s, rows, upstream, elapsed)

    def test_budget_is_not_multiplied_by_duplicate_actor_rows(self):
        s, rows, upstream, elapsed = fixture()
        rows[0]['actors'] *= 2
        with self.assertRaises(AssertionError): C.audit_schedule(s, rows, upstream, elapsed)


if __name__ == '__main__':
    unittest.main()
