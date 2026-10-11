import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from test_initial_cover import fixture as initial_fixture

spec=importlib.util.spec_from_file_location('handoff',Path(__file__).parents[1]/'validate-covered-handoff.py')
H=importlib.util.module_from_spec(spec);spec.loader.exec_module(H)


def fixture():
    row,local,site=initial_fixture()
    p=local['combat']['recovery']['flight']['pilot'];p['site_query']=dict(selected=site['id'])
    row['pilot']['site_query']=p['site_query']
    local['combat']['recovery']['flight']['flight']=dict(enabled=True)
    objective=dict(planet=0,revision=0,owner='player_2',position=dict(x=0.,y=60.),range=2.8)
    receipt=dict(tick=12,site=site['id'],previous_generation=8,previous_request_tick=10,
        previous_measurement_tick=10,previous_graph=5,previous_physics_queries=19,deadline_tick=130)
    evidence=dict(tick=12,generation=9,request_tick=12,measurement_tick=12,objective=objective,
        source_objective=objective,invalidated_by=None,submission_deferred_by=None,publication=None,covered_handoff=receipt)
    row['objective_evidence']=local['objective_evidence']=evidence
    row['objective_work']=local['objective_work']='pending'
    row['covered_handoff']=row['initial_cover']
    source=copy.deepcopy(row);source.pop('covered_handoff');source.pop('initial_cover')
    source['pilot'].update(tick=10,site_query='survey')
    source['objective_evidence']=dict(copy.deepcopy(evidence),tick=10,generation=8,request_tick=10,measurement_tick=10)
    source['objective_evidence'].pop('covered_handoff')
    return row,source,dict(graph=5,physics_queries=19)


class CoveredHandoffTests(unittest.TestCase):
    def test_receipt_binds_both_generations_clocks_and_retired_work(self):
        row,source,spent=fixture()
        h=H.audit_receipt(row,source,spent,11)
        self.assertEqual(h['deadline_tick'],130)
        self.assertEqual(row['objective_evidence']['measurement_tick'],12)

    def test_old_clock_cannot_be_relabelled_or_deadline_renewed(self):
        for mutation in range(3):
            row,source,spent=fixture();e=row['objective_evidence']
            if mutation==0:e['measurement_tick']=10
            elif mutation==1:e['covered_handoff']['deadline_tick']=132
            else:e['generation']=8
            with self.assertRaises(AssertionError):H.audit_receipt(row,source,spent,11)

    def test_missing_or_negative_cover_cannot_authorize_handoff(self):
        for key in ['grounded','approach']:
            row,source,spent=fixture();row['cover'][0][key]=False
            with self.assertRaises(AssertionError):H.audit_receipt(row,source,spent,11)

    def test_targeted_parent_and_wrong_objective_are_not_ordinary_sources(self):
        for mutation in range(3):
            row,source,spent=fixture()
            if mutation==0:source['pilot']['site_query']=row['pilot']['site_query']
            elif mutation==1:source['objective_evidence']['objective']['position']['x']=1.
            else:source['objective_evidence']['objective']['revision']=1
            with self.assertRaises(AssertionError):H.audit_receipt(row,source,spent,11)

    def test_retired_work_is_exact_and_cannot_continue_after_handoff(self):
        row,source,spent=fixture()
        for work,tick in [(dict(graph=4,physics_queries=19),11),(spent,12)]:
            with self.assertRaises(AssertionError):H.audit_receipt(row,source,work,tick)

    def test_command_preserves_initial_and_health_options(self):
        old=dict(item=dict(seat=1),command=['old','--out','old-dir','--initial-cover-seats','1','--pursuit-health-seats','1'])
        off=H.command(old,'frozen','new-dir',False)
        self.assertEqual(H.command(old,'frozen','new-dir',True),off+['--covered-request-handoff-seats','1'])
        self.assertEqual(off[3:],old['command'][3:])

    def test_complete_audit_counts_event_once_and_checks_dispatch_ledger(self):
        row,source,spent=fixture()
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'report.json').write_text(json.dumps(dict(live_objective_planning=dict(
                covered_request_handoff=dict(profile=H.PROFILE,enabled_seats=[0]),
                telemetry=dict(covered_request_handoffs={'0':1})))))
            # Remove the unrelated initial witness on the source; this auditor
            # separately binds only the scheduler's handoff observation.
            (root/'capture-evidence.jsonl').write_text('\n'.join(map(json.dumps,[source,row]))+'\n')
            ledger='tick,actor,generation,graph,queries,task\n10,0,8,2,7,landing_objective\n11,0,8,3,12,landing_objective\n12,0,9,4,384,landing_objective\n'
            (root/'live-planning.csv').write_text(ledger)
            result=H.audit_handoffs(root,dict(seat=0),True)
            self.assertEqual(result['count'],1);self.assertEqual(result['retired_work'],spent)
            self.assertEqual(result['publications'],0)
            (root/'live-planning.csv').write_text(ledger+'12,0,8,1,0,landing_objective\n')
            with self.assertRaises(AssertionError):H.audit_handoffs(root,dict(seat=0),True)


if __name__=='__main__':unittest.main()
