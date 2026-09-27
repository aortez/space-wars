import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('schedule', Path(__file__).resolve().parents[1] / 'schedule-transfer-references.py')
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)


def fixture():
    case = dict(source_tick=10, destination=1, seat=0)
    token = dict(actor=0, generation=1)
    state = dict(token=token, actor='player_1', source_tick=10, destination=1,
                 validated_tick=10, completed_tick=None, cancelled_tick=None,
                 phase='pending', reason=None, charged_graph=2)
    ready = dict(state, validated_tick=11, completed_tick=11, phase='ready', charged_graph=3)
    stale = dict(ready, validated_tick=None, cancelled_tick=12, phase='stale', reason='physical contact')
    report = dict(charged_graph=3, source_tick=10, handoff_seconds=.05)
    expected = dict(environment={'tick': 10}, source_actions=['thrust'], report=report)
    schedule = dict(total_graph_allowance=4, playing_graph_allowance=4, max_source_age_ticks=120,
        observational=True, physics_queries=0, source_environment=expected['environment'],
        source_actions=expected['source_actions'], published=report, published_state=ready,
        final_state=stale, charged_graph=3, completed=1, cancelled=1, submitted=1, rejected=None)
    rows = []
    for n, (s, prior, charge) in enumerate([(state, 2, 2), (ready, 3, 1), (stale, 0, 0)]):
        allowance = dict(graph=4-prior, physics_queries=0)
        charged = dict(graph=charge, physics_queries=0)
        rows.append(dict(event='dispatch', tick=10+n, playing_graph_allowance=4,
            playing_charged_graph=prior, total_graph_allowance=4, remaining_before_forecast=allowance,
            allocation=dict(tick=n+1, allowance=allowance, charged=charged,
                jobs=[] if n == 2 else [dict(request=token, charged=charged, phase=s['phase'].title())]),
            state=s, rejected=None, observation_ms=.001, dispatch_ms=.001))
    rows.append(dict(event='finish', tick=20, state=stale))
    upstream = {r['tick']: dict(total=r['playing_charged_graph']) for r in rows[:-1]}
    return case, schedule, rows, expected, upstream


class SchedulingAudit(unittest.TestCase):
    def test_independent_upstream_ledger_includes_shadow_and_checks_jobs(self):
        work = lambda graph, queries=0: dict(graph=graph, physics_queries=queries)
        e = dict(tick=0, remaining_before_evaluation=work(3, 382), allowance=work(3), charged=work(1))
        f = dict(tick=0, remaining_after_evaluation=work(2, 382),
                 allocation=dict(allowance=work(2, 382), charged=work(1, 2), jobs=[dict(charged=work(1, 2))]))
        s = dict(tick=0, remaining_after_flag_survey=work(1), charged=work(1))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'live-planning.csv').write_text('tick,graph_budget,query_budget,total_graph,total_queries,graph,queries\n0,4,384,1,2,1,2\n')
            for name, row in [('mission-evaluation', e), ('flag-survey', f), ('flag-value-shadow', s)]:
                (root/(name+'-work.jsonl')).write_text(json.dumps(row)+'\n')
            (root/'report.json').write_text(json.dumps(dict(elapsed_ticks=1, mission_evaluation=dict(charged=1),
                flag_survey=dict(shadow=dict(charged=1), telemetry=work(1, 2)),
                live_objective_planning=dict(telemetry=work(1, 2)))))
            self.assertEqual(S.audit_upstream(root)[0], dict(live=1, evaluator=1, flags=1, shadow=1, total=4))
            s['charged'] = work(2)
            (root/'flag-value-shadow-work.jsonl').write_text(json.dumps(s)+'\n')
            with self.assertRaises(AssertionError): S.audit_upstream(root)

    def test_completion_then_cancellation_preserves_historical_duration(self):
        args = fixture()
        result = S.audit_lifetime(*args)
        self.assertTrue(result['completed'])
        self.assertEqual(result['completion_age_ticks'], 1)
        self.assertEqual(result['cancellation_reason'], 'physical contact')
        self.assertEqual(args[1]['published']['handoff_seconds'], .05)
        summary = S.aggregate({'a': dict(allowance=4, schedule=result)})['4']
        self.assertEqual(summary['completed'], 1)
        self.assertEqual(summary['cancellation_reasons'], {'physical contact': 1})

    def test_rejects_double_spent_work_unreconciled_ledger_and_query_work(self):
        for change in ['total', 'upstream', 'allowance', 'query', 'job', 'job_token']:
            case, schedule, rows, expected, upstream = copy.deepcopy(fixture())
            if change == 'total': schedule['charged_graph'] += 1
            elif change == 'upstream': upstream[10]['total'] += 1
            elif change == 'allowance': rows[0]['remaining_before_forecast']['graph'] += 1
            elif change == 'query': rows[0]['allocation']['charged']['physics_queries'] = 1
            elif change == 'job': rows[0]['allocation']['jobs'] = []
            else: rows[0]['allocation']['jobs'][0]['request'] = dict(actor=1, generation=1)
            with self.subTest(change=change), self.assertRaises(AssertionError):
                S.audit_lifetime(case, schedule, rows, expected, upstream)

    def test_rejects_late_publication_rebased_output_or_work_after_cancel(self):
        for change in ['future', 'invalid_ready', 'rebase', 'cancel_work', 'premature_expiry', 'source', 'duplicate_tick']:
            case, schedule, rows, expected, upstream = copy.deepcopy(fixture())
            if change == 'future': rows[1]['state']['completed_tick'] = 12
            elif change == 'invalid_ready': rows[1]['state']['validated_tick'] = 10
            elif change == 'rebase': schedule['published'] = dict(expected['report'], handoff_seconds=.04)
            elif change == 'cancel_work': rows[2]['allocation']['charged']['graph'] = 1
            elif change == 'premature_expiry': rows[2]['state']['reason'] = 'source expired'
            elif change == 'source': schedule['source_environment'] = {'tick': 9}
            else: rows[1]['tick'] = 10
            with self.subTest(change=change), self.assertRaises(AssertionError):
                S.audit_lifetime(case, schedule, rows, expected, upstream)

    def test_finish_cannot_rewrite_or_backdate_a_cancellation(self):
        for tick in [9, 11, 15]:
            case, schedule, rows, expected, upstream = copy.deepcopy(fixture())
            schedule['final_state'] = dict(schedule['final_state'], cancelled_tick=tick, reason='source expired')
            rows[-1]['state'] = schedule['final_state']
            with self.subTest(tick=tick), self.assertRaises(AssertionError):
                S.audit_lifetime(case, schedule, rows, expected, upstream)

    def test_starved_pending_jobs_expire_without_success_credit(self):
        case, schedule, _, expected, _ = fixture()
        rows, upstream = [], {}
        for age in range(122):
            tick = case['source_tick'] + age
            expired = age == 121
            state = dict(token=dict(actor=0, generation=1), actor='player_1', source_tick=10,
                destination=1, validated_tick=None if expired else tick, completed_tick=None,
                cancelled_tick=tick if expired else None, phase='stale' if expired else 'pending',
                reason='source expired' if expired else None, charged_graph=0)
            work = dict(graph=0, physics_queries=0)
            rows.append(dict(event='dispatch', tick=tick, playing_graph_allowance=4, playing_charged_graph=4,
                total_graph_allowance=4, remaining_before_forecast=work,
                allocation=dict(tick=age+1, allowance=work, charged=work,
                    jobs=[] if expired else [dict(request=state['token'], phase='Pending', charged=work)]),
                state=state, rejected=None, observation_ms=0, dispatch_ms=0))
            upstream[tick] = dict(total=4)
        rows.append(dict(event='finish', tick=200, state=state))
        schedule.update(published=None, published_state=None, final_state=state, charged_graph=0, completed=0)
        result = S.audit_lifetime(case, schedule, rows, expected, upstream)
        self.assertFalse(result['completed'])
        self.assertEqual(result['zero_work_pending_ticks'], 121)
        summary = S.aggregate({'a': dict(allowance=4, schedule=result)})['4']
        self.assertEqual(summary['completed'], 0)
        self.assertIsNone(summary['mean_completion_age_ticks'])
        self.assertEqual(summary['cases'], 1)


if __name__ == '__main__':
    unittest.main()
