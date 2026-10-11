import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

spec=importlib.util.spec_from_file_location('sweep_analysis',Path(__file__).parents[1]/'analyze-projectile-response-sweep.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def record(seat,changed=False):
    return dict(round=dict(outcome=dict(winner=f'player_{seat+1}'),pilots=[dict(death_tick=None)]*2),
                physical=dict(claimed=2,departed=1),coverage=dict(ready_ticks=3),attempt={} if changed else None,
                first_action_change=2 if changed else None,parity=None if changed else {})


class ResponseSweepAnalysisTests(unittest.TestCase):
    def test_prior_experiment_is_identified_by_schema_without_requiring_a_filename(self):
        speed=dict(plan=[dict(enabled=True)],runs={})
        response=dict(plan=[dict(mode='observe')],runs={})
        self.assertIs(A.response_source([speed,response]),response)
        with self.assertRaises(AssertionError):A.response_source([response,response])

    def test_p2_wins_and_health_cases_are_not_pooled_with_primary(self):
        cases=[dict(key='p2',group='fresh',item=dict(seat=1)),dict(key='health',group='health',item=dict(seat=0))]
        records={c['key']+'-'+mode:record(c['item']['seat'],mode=='left') for c in cases for mode in ['observe','brake','left']}
        groups,paired=A.comparisons(cases,records)
        self.assertEqual(set(groups),{'fresh','health'})
        self.assertEqual(groups['fresh']['observe']['wins'],1)
        self.assertEqual(groups['fresh']['brake']['changed_cases'],0)
        self.assertEqual(groups['fresh']['left']['changed_cases'],1)
        self.assertEqual(paired['p2']['modes']['left']['delta']['claims'],0)

    def test_untriggered_case_keeps_parity_and_does_not_invent_contact_evidence(self):
        with TemporaryDirectory() as directory:
            root=Path(directory)
            report=dict(elapsed_ticks=10,round={},missions=[dict(escape_travel=dict(attempts=0,last=None))],
                        final_pilots=[dict(recovery=dict(ships_lost=0))],samples=[{}])
            (root/'report.json').write_text(json.dumps(report))
            (root/'projectile-response-witnesses.json').write_text(json.dumps(dict(rows=[],final={})))
            run=dict(command=['binary','--out',directory],item=dict(seat=0),visits={'0':[]},visit_audit={},
                     allocation={},projectiles={},parity=dict(exact=True),
                     response=dict(attempt=None,first_action_change=None,exact_prefix_rows=20))
            summary,evidence=A.A.analyze(dict(mode='left',source='case'),run)
            self.assertIsNone(summary['attempt']);self.assertEqual(summary['exact_prefix_rows'],20)
            self.assertNotIn('triggered_projectile_contacts',summary)
            self.assertEqual(evidence['response_witnesses']['rows'],[])


if __name__=='__main__':unittest.main()
