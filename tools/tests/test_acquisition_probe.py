import copy
import importlib.util
import json
from pathlib import Path
import unittest
import tempfile

SPEC = importlib.util.spec_from_file_location('acquisition_probe', Path(__file__).resolve().parents[1] / 'probe-site-acquisition.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


def row(tick, site=None):
    p = dict(tick=tick, owner='player_1', ship_available=True, ship_health=100,
             ship_form='ship', location='aboard', planet=dict(index=1))
    capture = dict(site=site, started_tick=11 if tick > 10 else None, replans=0,
        failed_tick=None, failure=None, completed_tick=None, landing=dict(landed_tick=None),
        acquisition=dict(tick=tick, planet=1, selected_site=site, reason='selected_site' if site else 'survey_deferred'))
    m = dict(capture=capture, target=1, goal='capture', recovery=None,
             events=[dict(tick=10, planet=1, kind='arrived')])
    local = dict(combat=dict(recovery=dict(flight=dict(pilot=p))))
    return dict(tick=tick, seat=0, observation=dict(local=local), mission=m)


def acquisition(rows, reason, observed=True):
    end = rows[-1]['tick'] + (not observed)
    return dict(transfer_probe=dict(outcome=dict(tick=10, reason='arrived'),
        acquisition=dict(schema=1, started_tick=10, horizon_ticks=1800, initial_replans=0, last_observed_tick=rows[-1]['tick'],
                         observed_rows=len(rows), outcome=dict(tick=end, elapsed_ticks=end-10,
                            reason=reason, controller_observed=observed, site=None))),
                elapsed_ticks=end, round=dict(outcome='draw' if reason == 'match_finished' else None))


def fixture():
    site = dict(planet=1, bearing=2)
    evidence = dict(site=site, revision=1, observed_owner=None, radius=50,
        stage_seconds=3, flag_range=2.8, remote=True, tick=9, age_ticks=1, gravity=0,
        route_source_tick=None, route_validated_tick=None, route_objective=None, choice=None)
    costs = dict.fromkeys(P.L.PHASES, 1)
    reference = dict(model='source_local_reference_v1', source_tick=10, destination=1,
        visit_tick=10, selected_site=None, observed_choice_tick=None, elapsed_landing_ticks=0,
        evidence=evidence, full=costs, remaining=costs, unknown=None)
    planet = dict(index=1, revision=1, radius=50, claim=dict(owner=None, flag=None,
        stage_required_seconds=3, flag_interaction_range=3), motion=dict(angle=0, position=dict(x=0, y=0)))
    rows = [row(10), row(11, site)]
    for r in rows: r['observation']['planets'] = [copy.deepcopy(planet)]
    case = dict(seat=0, source_tick=10, destination=1, candidate=dict(local_reference=reference))
    report = acquisition(rows, 'site_selected')
    report['transfer_probe']['acquisition']['outcome']['site'] = site
    return case, report, rows


class AcquisitionProbeAudit(unittest.TestCase):
    def test_evaluator_work_is_bound_to_both_budget_residual_and_report(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            live = [dict(tick='0', graph_budget='4', query_budget='384', total_graph='1',
                         total_queries='2', graph='1', queries='2')]
            flags = [dict(tick=0, remaining_after_evaluation=dict(graph=2, physics_queries=382)),
                     dict(tick=1, remaining_after_evaluation=dict(graph=0, physics_queries=384))]
            write = lambda: (root/'flag-survey-work.jsonl').write_text(''.join(json.dumps(f)+'\n' for f in flags))
            (root/'report.json').write_text(json.dumps(dict(elapsed_ticks=2, mission_evaluation=dict(charged=5),
                live_objective_planning=dict(telemetry=dict(graph=1, physics_queries=2)))))
            write()
            self.assertEqual(P.evaluation_charges(root, live), [1, 4])
            flags[0]['remaining_after_evaluation']['graph'] = 1
            write()
            with self.assertRaises(AssertionError): P.evaluation_charges(root, live)
            flags[0]['remaining_after_evaluation']['graph'] = 2
            flags[1]['remaining_after_evaluation']['physics_queries'] = 383
            write()
            with self.assertRaises(AssertionError): P.evaluation_charges(root, live)

    def test_local_restart_or_frame_change_cannot_count_as_original_choice(self):
        case, report, rows = fixture()
        rows[-1]['mission']['capture']['replans'] = 1
        self.assertEqual(P.endpoint(rows[-1], 10, 1, 1800), 'capture_replanned')
        with self.assertRaises(AssertionError): P.audit_acquisition(case, report, rows)
        rows[-1]['mission']['capture']['replans'] = 0
        P.pilot(rows[-1])['planet']['index'] = 0
        self.assertEqual(P.endpoint(rows[-1], 10, 1, 1800), 'approach_frame_changed')
        with self.assertRaises(AssertionError): P.audit_acquisition(case, report, rows)

    def test_complete_choice_and_censored_match_have_distinct_intervals(self):
        case, report, rows = fixture()
        result = P.audit_acquisition(case, report, rows)
        self.assertEqual(result['choice_seconds'], 1/60)
        self.assertTrue(result['compatible_reference_at_choice'])
        self.assertEqual(result['reference']['evidence_age_at_arrival'], 1)
        self.assertEqual(result['reference']['evidence_age_at_endpoint'], 2)
        self.assertEqual(result['waiting_ticks'], 1)
        rows = rows[:1]
        report = acquisition(rows, 'match_finished', observed=False)
        result = P.audit_acquisition(case, report, rows)
        self.assertIsNone(result['choice_seconds'])
        self.assertEqual(result['waiting_ticks'], 1)
        self.assertEqual(result['reference']['evidence_age_at_last_observation'], 1)
        self.assertEqual(result['reference']['evidence_age_at_endpoint'], 2)
        self.assertIsNone(result['reference']['endpoint_planet'])

    def test_same_bearing_does_not_renew_changed_or_expired_evidence(self):
        case, report, rows = fixture()
        rows[-1]['observation']['planets'][0]['revision'] += 1
        result = P.audit_acquisition(case, report, rows)
        self.assertTrue(result['matches_reference_site'])
        self.assertFalse(result['compatible_reference_at_choice'])
        self.assertEqual(result['reference']['first_incompatible_tick'], 11)
        rows[-1]['observation']['planets'][0]['revision'] -= 1
        later = copy.deepcopy(rows[-1])
        later['tick'] = 1810
        status = P.reference_status(case, rows[0], [rows[0], later])
        self.assertEqual(status['first_expired_tick'], 1810)
        self.assertEqual(status['evidence_age_at_last_observation'], 1801)

    def test_reference_site_mismatch_preserves_observed_time(self):
        case, report, rows = fixture()
        case['candidate']['local_reference']['evidence']['site'] = dict(planet=1, bearing=3)
        result = P.audit_acquisition(case, report, rows)
        self.assertEqual(result['choice_seconds'], 1/60)
        self.assertFalse(result['matches_reference_site'])
        self.assertFalse(result['compatible_reference_at_choice'])

    def test_fresh_choice_needs_actor_attempt_and_native_clock(self):
        site = dict(planet=1, bearing=2)
        r = row(11, site)
        self.assertEqual(P.first_choice(r, 10, 1), site)
        for mutate in [lambda c: c.update(started_tick=9),
                       lambda c: c.update(started_tick=12),
                       lambda c: c['acquisition'].update(tick=10),
                       lambda c: c['acquisition'].update(planet=0),
                       lambda c: c['acquisition'].update(selected_site=None),
                       lambda c: c.update(acquisition=None)]:
            changed = copy.deepcopy(r)
            mutate(changed['mission']['capture'])
            self.assertIsNone(P.first_choice(changed, 10, 1))
            self.assertEqual(P.endpoint(changed, 10, 1, 1800), 'unwitnessed_site')

    def test_recovery_and_replaced_attempts_beat_a_same_tick_choice(self):
        r = row(11, dict(planet=1, bearing=2))
        r['mission']['goal'] = 'recover'
        self.assertEqual(P.endpoint(r, 10, 1, 1800), 'recovery')
        r['mission']['goal'] = 'capture'
        for kind in ['selected', 'replan', 'arrived']:
            r['mission']['events'] = [dict(tick=11, planet=1, kind=kind)]
            self.assertEqual(P.endpoint(r, 10, 1, 1800), 'attempt_replaced')

    def test_deadline_choice_is_observed_but_loss_still_precedes_it(self):
        r = row(1810, dict(planet=1, bearing=2))
        self.assertEqual(P.endpoint(r, 10, 1, 1800), 'site_selected')
        P.pilot(r)['ship_health'] = 0
        self.assertEqual(P.endpoint(r, 10, 1, 1800), 'ship_or_pilot_lost')
        self.assertEqual(P.endpoint(row(1810), 10, 1, 1800), 'observation_horizon')

    def test_solar_avoidance_retains_the_attempt_and_cannot_reuse_stale_evidence(self):
        r = row(11)
        r['mission']['goal'] = 'avoid_sun'
        r['mission']['capture']['acquisition']['tick'] = 10
        self.assertIsNone(P.endpoint(r, 10, 1, 1800))
        r['mission']['capture']['site'] = dict(planet=1, bearing=2)
        self.assertEqual(P.endpoint(r, 10, 1, 1800), 'unwitnessed_site')

    def test_physical_progress_without_a_choice_is_not_zero_wait_success(self):
        for field in ['landed', 'on_foot', 'complete']:
            r = row(11)
            if field == 'landed': r['mission']['capture']['landing']['landed_tick'] = 11
            if field == 'on_foot': P.pilot(r)['location'] = 'on_foot'
            if field == 'complete': r['mission']['capture']['completed_tick'] = 11
            self.assertEqual(P.endpoint(r, 10, 1, 1800), 'physical_progress_without_choice')

    def test_failed_transfer_does_not_generate_an_acquisition_sample(self):
        report = dict(transfer_probe=dict(outcome=dict(tick=20, reason='solver_or_debris_contact'),
            acquisition=dict(schema=1, horizon_ticks=1800, started_tick=None,
                             last_observed_tick=None, outcome=None, observed_rows=0)))
        self.assertIsNone(P.audit_acquisition({}, report, [])['choice_seconds'])
        report['transfer_probe']['acquisition']['started_tick'] = 20
        with self.assertRaises(AssertionError): P.audit_acquisition({}, report, [])

    def test_endpoint_cannot_hide_a_prior_choice_or_restart(self):
        rows = [row(10), row(11, dict(planet=1, bearing=2)), row(12)]
        report = acquisition(rows, 'capture_ended')
        rows[-1]['mission']['capture'] = None
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)
        rows[1] = row(11)
        rows[1]['mission']['events'].append(dict(tick=11, planet=1, kind='replan'))
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)

    def test_dense_ledger_rejects_a_gap_or_wrong_actor(self):
        rows = [row(10), row(12)]
        report = acquisition(rows, 'capture_ended')
        rows[-1]['mission']['capture'] = None
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)
        rows.insert(1, row(11))
        report = acquisition(rows, 'capture_ended')
        rows[1]['seat'] = 1
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)

    def test_synthetic_finish_cannot_report_a_choice(self):
        rows = [row(10), row(11)]
        report = acquisition(rows, 'site_selected', observed=False)
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)
        report['transfer_probe']['acquisition']['outcome']['reason'] = 'match_finished'
        report['transfer_probe']['acquisition']['outcome']['controller_observed'] = True
        with self.assertRaises(AssertionError): P.audit_acquisition(dict(seat=0, destination=1), report, rows)

    def test_aggregate_never_converts_missing_choice_to_zero_time(self):
        runs = dict(a=dict(acquisition=dict(started=False, transfer_outcome='timeout', choice_seconds=None)),
                    b=dict(acquisition=dict(started=True, outcome=dict(reason='observation_horizon'), choice_seconds=None)),
                    c=dict(acquisition=dict(started=True, outcome=dict(reason='site_selected'), choice_seconds=1/60,
                                            matches_reference_site=False, compatible_reference_at_choice=False)))
        result = P.aggregate(runs)
        self.assertEqual(result['completed_choice_seconds'], [1/60])
        self.assertEqual(result['cases'], 3)
        self.assertEqual(result['compatible_reference_at_choice'], 0)


if __name__ == '__main__':
    unittest.main()
