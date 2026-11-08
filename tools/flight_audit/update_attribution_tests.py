"""Exact production-update replay, selection and graded-reward instrumentation."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import sha
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.reward import reward
from .update_attribution import PARENT, VARIANTS, grouped_rows, initial_probes, probe, replay_case, select_updates


class UpdateAttributionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        location = os.environ.get('FLIGHT_UPDATE_ATTRIBUTION_TEST_ARTIFACTS')
        cls.temporary = None
        if location:
            cls.root = Path(location);cls.root.mkdir()
        else:
            cls.temporary = tempfile.TemporaryDirectory();cls.root = Path(cls.temporary.name)
        with Bridge(PARENT/'runtime/flight_ppo_bridge') as bridge:
            admission = json.loads((PARENT/'seed-43/development-admission.json').read_text())
            _, items, cls.observations = initial_probes(bridge, admission)
            (cls.root/'probe-inputs.json').write_text(json.dumps(items)+'\n')
            cls.schema = bridge.contract['schema']

    @classmethod
    def tearDownClass(cls):
        if cls.temporary:cls.temporary.cleanup()

    def test_selection_bounds_ties_and_deduplication(self):
        scan = [{'update': n, 'mean_thrust_logit': -3. if n in [150,200] else 0.} for n in range(128,257)]
        chosen = select_updates(scan)
        self.assertEqual(chosen['updates'],[150,151,256])
        self.assertEqual(chosen['largest_drop']['update'],150)
        with self.assertRaises(ValueError):select_updates(scan[:-1])
        scan[50]['mean_thrust_logit'] = float('nan')
        with self.assertRaises(ValueError):select_updates(scan)

    def test_probe_is_rng_and_parameter_neutral(self):
        with torch.random.fork_rng(devices=[]):
            model = FlightModel(self.schema)
            before = copy.deepcopy(model.state_dict());rng = torch.get_rng_state()
            a, b = probe(model,self.observations), probe(model,self.observations)
            self.assertEqual(a,b);self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            self.assertTrue(all(torch.equal(v,before[k]) for k,v in model.state_dict().items()))

    def test_graded_deadline_is_present_in_row_instrumentation(self):
        frame = {'errors': {'position':10.,'velocity':0.,'spin':0.},
                 'target_position':{'x':0.,'y':10.},'center_of_mass':{'x':0.,'y':0.},'angle':0.}
        after = {'frame':copy.deepcopy(frame),'progress':{'outcome':'deadline'}}
        row = {'slot':0,'episode':0,'tick':1799,'before':frame,'after':after,
               'action':{'steering':0.,'thrust':False,'brake':True}}
        config = {'gamma':.9995,'velocity_shaping_weight':.25,'deadline_distance_weight':4.}
        total = reward({'frame':frame},after,.9995,.25,4.)['total']
        rollout = {'advantages':torch.zeros(1,1),'returns':torch.zeros(1,1),
                   'rewards_audit':torch.tensor([[total]]),'ar_latents':torch.zeros(1,1,3)}
        _, rows = grouped_rows([row],rollout,torch.zeros(1),torch.zeros(1),torch.zeros(1,3),config)
        self.assertAlmostEqual(rows[0]['reward']['terminal'],-2.-4.*7./37.)

    def test_real_update_reconstructs_exactly_and_retains_coefficient_probes(self):
        source = PARENT/'seed-43';paths = [source/f'checkpoint-{n:04}.pt' for n in [128,129]]
        hashes = [sha(p) for p in paths]
        result = replay_case(source,PARENT/'runtime/flight_ppo_bridge',self.root/'update-0129',129,self.observations)
        self.assertTrue(result['exact_archive_replay'])
        self.assertTrue(result['rewards_values_gae_targets_reconstructed_exactly'])
        self.assertEqual(set(result['branches']),set(VARIANTS))
        self.assertLessEqual(result['replay_ticks_including_restore'],8300)
        self.assertLessEqual(result['diagnostic_optimizer_steps'],64)
        self.assertEqual([sha(p) for p in paths],hashes)


if __name__ == '__main__':
    unittest.main(verbosity=2)
