import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('selection_trials',Path(__file__).parents[1]/'validate-projectile-selection.py')
T=importlib.util.module_from_spec(spec);spec.loader.exec_module(T)


class SelectionTrialTests(unittest.TestCase):
    def test_archive_comparison_normalizes_seats_without_hiding_changed_visits(self):
        current={0:[dict(claimed_tick=42)],1:[]};saved={'0':[dict(claimed_tick=42)],'1':[]}
        self.assertTrue(T.archived_equal(current,saved))
        current[0][0]['claimed_tick']=43;self.assertFalse(T.archived_equal(current,saved))

    def test_fresh_command_preserves_reporting_seat_and_sets_declared_conditions(self):
        old=dict(command=['old','--out','old-dir','--seed','42','--seat','0','--asteroid-interval','0',
                          '--probe-projectile-response','observe','--probe-projectile-response-seat','0',
                          '--probe-projectile-response-scope','transfer'])
        case=dict(new=True,scope='transfer',item=dict(seat=1,seed=123,asteroid_interval=3))
        cmd=T.command(case,old,'binary','out','guarded_brake')
        for flag,value in [('--seed','123'),('--seat','0'),('--asteroid-interval','3'),
                           ('--probe-projectile-response-seat','1'),('--probe-projectile-response','guarded_brake')]:
            self.assertEqual(cmd.count(flag),1);self.assertEqual(cmd[cmd.index(flag)+1],value)
        self.assertEqual(old['command'][old['command'].index('--seed')+1],'42')


if __name__=='__main__':unittest.main()
