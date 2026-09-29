import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('arrival_survey', Path(__file__).resolve().parents[1] / 'survey-predicted-arrivals.py')
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)


def fixture():
    token = dict(actor=0,generation=1)
    site = dict(planet=2,bearing=0)
    work = dict(graph=0,physics_queries=7)
    state = dict(phase='ready',validated_tick=8,token=token,source_tick=5,completed_tick=7)
    forecast = dict(source_tick=5,destination=2,model='guided_transfer_forecast_v1',ticks=100,
        end='kinematic_handoff',samples=[dict(after_ticks=100,
            ship=dict(position=dict(x=0,y=10)),target=dict(position=dict(x=0,y=0),angle=0))])
    pilot = dict(site_query='not_requested',queries_ready=True,landing=dict(supported_feet=0))
    measurement = dict(tick=8,queries=7,site=dict(id=site),finding='measured')
    plan = dict(token=token,source_tick=5,completed_tick=7,forecast_model=forecast['model'],
        predicted_arrival_tick=105,site=site,deferred=None,
        request=dict(generation=8,candidates=[site,None,None,None],sample_climb=True))
    row = dict(tick=8,seat=0,plan=plan,deferred=None,remaining_before=dict(graph=0,physics_queries=384),
        allocation=dict(allowance=dict(graph=0,physics_queries=384),charged=work,
            jobs=[dict(request=token,charged=work)]),
        evidence=[[0,dict(generation=8,candidates=[dict(id=site,status='measured',measurement=measurement)])]])
    return row,state,forecast,pilot


class ArrivalSurveyAudit(unittest.TestCase):
    def test_first_request_after_initial_deferral_cannot_be_backdated(self):
        row = fixture()[0]
        known = {}
        deferred = copy.deepcopy(row)
        deferred['plan']['request'] = None
        S.audit_request_generation(deferred,known)
        later = copy.deepcopy(row)
        later['tick'] = 10
        with self.assertRaises(AssertionError): S.audit_request_generation(later,known)
        later['plan']['request']['generation'] = 10
        S.audit_request_generation(later,known)
        S.audit_request_generation(deferred,known)
        later['tick'] = 11
        S.audit_request_generation(later,known)
        later['plan']['request'] = dict(later['plan']['request'],generation=11)
        with self.assertRaises(AssertionError): S.audit_request_generation(later,known)

    def test_current_sample_has_later_epoch_and_consumes_actual_query_charge(self):
        last = {}
        measurement,used = S.audit_row(*fixture(),384,[],last)
        self.assertEqual(used,7)
        self.assertEqual(measurement['tick'],8)
        self.assertEqual(last,{0:8})

    def test_same_tick_forecast_stale_token_backdated_or_wrong_site_evidence_fails(self):
        for change in ['completion','validation','phase','token','source','generation','measurement','site','model','budget','charge']:
            row,state,forecast,pilot = copy.deepcopy(fixture())
            if change == 'completion': row['plan']['completed_tick'] = state['completed_tick'] = 8
            elif change == 'validation': state['validated_tick'] = 7
            elif change == 'phase': state['phase'] = 'stale'
            elif change == 'token': state['token'] = dict(actor=0,generation=2)
            elif change == 'source': row['plan']['source_tick'] = 4
            elif change == 'generation': row['plan']['request']['generation'] = 6
            elif change == 'measurement': row['evidence'][0][1]['candidates'][0]['measurement']['tick'] = 5
            elif change == 'site': row['plan']['site'] = dict(planet=2,bearing=1)
            elif change == 'model': row['plan']['forecast_model'] = 'body_v2'
            elif change == 'budget': row['remaining_before']['physics_queries'] = 385
            else: row['evidence'][0][1]['candidates'][0]['measurement']['queries'] = 8
            with self.subTest(change=change), self.assertRaises(AssertionError):
                S.audit_row(row,state,forecast,pilot,384,[],{})

    def test_local_budget_busy_and_refresh_deferrals_cannot_carry_measurements(self):
        for reason in ['local landing demand','query budget','earlier physical work','refresh interval','material queries unavailable']:
            row,state,forecast,pilot = copy.deepcopy(fixture())
            remaining,busy,last = 384,[],{}
            if reason == 'local landing demand': pilot['site_query'] = 'all'
            if reason == 'material queries unavailable': pilot['queries_ready'] = False
            if reason in ['local landing demand','material queries unavailable']:
                row['plan'].update(request=None,deferred=reason)
            if reason == 'query budget': remaining = 191
            if reason == 'earlier physical work': busy = [0]
            if reason == 'refresh interval': last = {0:7}
            row.update(deferred=reason,allocation=None,evidence=[],remaining_before=dict(graph=0,physics_queries=remaining))
            self.assertEqual(S.audit_row(row,state,forecast,pilot,remaining,busy,last),(None,0))
            row['evidence'] = ['unexpected']
            with self.assertRaises(AssertionError): S.audit_row(row,state,forecast,pilot,remaining,busy,last)

    def test_negative_measurements_still_advance_actor_throttle(self):
        for finding in ['incomplete','no_landing']:
            row,state,forecast,pilot = copy.deepcopy(fixture())
            c = row['evidence'][0][1]['candidates'][0]
            c['status'] = c['measurement']['finding'] = finding
            c['measurement']['site'] = None
            last = {}
            self.assertEqual(S.audit_row(row,state,forecast,pilot,384,[],last)[1],7)
            self.assertEqual(last,{0:8})


if __name__ == '__main__': unittest.main()
