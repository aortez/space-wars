import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'recovery_rebuild', Path(__file__).resolve().parents[1] / 'validate-recovery-rebuild.py')
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


def receipt_pair():
    task = dict(started_tick=3220, landed_tick=4334, exited_tick=4336, relocations=4,
                relocation_surveys=6, ground_budget_ticks=0, scuttle_attempts=0,
                scuttle_started_tick=None, scuttled_tick=None, status='blocked', goal='blocked',
                reason='no accessible rebuild after four measured relocations')
    p = dict(tick=21947, owner='player_2', vehicle=1, location='on_foot', ship_form='escape_pod',
             ship_available=True, controls_armed=True, queries_ready=True,
             recovery=dict(rebuilds=0, ships_lost=1))
    old = dict(tick=21947, seat=1, actions=[],
               observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=p))))),
               mission=dict(recovery=task, completed_recoveries=0))
    new = copy.deepcopy(old)
    new['tick'] = 21948
    R.pilot(new).update(tick=21948, ship_form='ship', controls_armed=False,
                        recovery=dict(rebuilds=1, ships_lost=1))
    new['mission']['recovery'].update(goal='board', status='running', reason=None,
        site=None, relocation_site=None, ground=None, rebuild_boarding=dict(
            native_rebuilds=1, started_tick=21948, deadline_tick=27348,
            previous_goal=task['goal'], previous_reason=task['reason']))
    return old, new


class RecoveryRebuildAuditTests(unittest.TestCase):
    def test_commands_change_only_binary_and_output_for_both_recorded_arms(self):
        sources = {name: dict(item=dict(name=name, seed=11223442104665788832, defense=False,
                                       clearance=False, laser=i == 0),
                             command=['old-binary', '--trace', 'true', '--out', 'old-output',
                                      '--pursuit-climb-laser-seats', '0' if i == 0 else 'none'])
                   for i, name in enumerate(R.CASES)}
        original = copy.deepcopy(sources)
        jobs = R.jobs(sources, Path('/new-binary'), Path('/new-output'))
        self.assertEqual(sources, original)
        for name, job in zip(R.CASES, jobs):
            expected = list(original[name]['command'])
            expected[0], expected[4] = '/new-binary', f'/new-output/raw/{name}'
            self.assertEqual(job, dict(item=original[name]['item'], command=expected))

    def test_duplicate_command_options_are_rejected(self):
        sources = {name: dict(item=dict(seed=11223442104665788832, defense=False, clearance=False),
                             command=['old', '--out', 'a', '--out', 'b']) for name in R.CASES}
        with self.assertRaises(AssertionError):
            R.jobs(sources, Path('/new'), Path('/out'))

    def test_parity_normalizes_only_task_version(self):
        row = dict(task='recover_ship_v10', reason='recover_ship_v10',
                   children=[dict(task='recover_ship_v10', rebuild_boarding={'started_tick': 123})])
        value = R.normalize_version(row)
        self.assertEqual(value['task'], 'recover_ship_v9')
        self.assertEqual(value['reason'], 'recover_ship_v10')
        self.assertEqual(value['children'][0]['rebuild_boarding'], {'started_tick': 123})
        self.assertEqual(row['task'], 'recover_ship_v10')

    def test_native_handoff_preserves_history_and_binds_generation(self):
        old, new = receipt_pair()
        receipt = R.check_receipt(new, old)
        self.assertEqual(receipt['task_started_tick'], 3220)
        self.assertEqual(receipt['receipt']['deadline_tick'], 27348)
        self.assertEqual(receipt['ships_lost'], 1)
        self.assertIsNone(receipt['completed_tick'])

    def test_handoff_rejects_replayed_progress_and_reset_budgets(self):
        for fault in ('counter', 'form', 'available', 'owner', 'vehicle', 'start', 'relocations',
                      'scuttles', 'deadline', 'prior_receipt', 'prior_reason', 'active_route'):
            with self.subTest(fault=fault):
                old, new = receipt_pair()
                p, t = R.pilot(new), new['mission']['recovery']
                if fault == 'counter': p['recovery']['rebuilds'] = 0
                elif fault == 'form': p['ship_form'] = 'escape_pod'
                elif fault == 'available': p['ship_available'] = False
                elif fault == 'owner': p['owner'] = 'player_1'
                elif fault == 'vehicle': p['vehicle'] = 0
                elif fault == 'start': t['started_tick'] = new['tick']
                elif fault == 'relocations': t['relocations'] = 0
                elif fault == 'scuttles': t['scuttle_attempts'] = 1
                elif fault == 'deadline': t['rebuild_boarding']['deadline_tick'] += 1
                elif fault == 'prior_receipt': old['mission']['recovery']['rebuild_boarding'] = {}
                elif fault == 'prior_reason': t['rebuild_boarding']['previous_reason'] = None
                else: t['ground'] = {'goal': 'arrived'}
                with self.assertRaises(AssertionError):
                    R.check_receipt(new, old)


if __name__ == '__main__':
    unittest.main()
