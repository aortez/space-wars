import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('ordinary',Path(__file__).parents[1]/'validate-ordinary-transfer-response.py')
O=importlib.util.module_from_spec(spec);spec.loader.exec_module(O)


class OrdinaryTransferResponseTests(unittest.TestCase):
    def test_scope_replacement_keeps_all_policy_physics_and_reporting_options(self):
        old=dict(command=['old','--out','before','--seat','0','--seed','42','--trace-projectiles','true',
                         '--probe-projectile-response','observe','--probe-projectile-response-seat','0'])
        cmd=O.command(old,'new','after','left','transfer',1)
        self.assertEqual(cmd.count('--probe-projectile-response'),1)
        self.assertEqual(cmd[cmd.index('--probe-projectile-response-seat')+1],'1')
        self.assertEqual(cmd[cmd.index('--seat')+1],'0')
        self.assertEqual(cmd[cmd.index('--seed')+1],'42')
        self.assertEqual(cmd[cmd.index('--probe-projectile-response-scope')+1],'transfer')
        legacy=O.command(dict(command=cmd),'new','legacy','observe','escape',1)
        self.assertNotIn('--probe-projectile-response-scope',legacy)


if __name__=='__main__':unittest.main()
