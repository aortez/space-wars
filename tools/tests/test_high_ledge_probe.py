import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('high_ledge',Path(__file__).resolve().parents[1]/'probe-high-ledge.py')
H=importlib.util.module_from_spec(spec); spec.loader.exec_module(H)


def candidates():
    return [dict(margin=m,height=h,ordinary_height_allowed=False,corridor_clear=False,
        plan=dict(anchor={'GroundGap':{'from':276,'to':282}},planet=0,revision=21,
                  start=dict(x=5,y=-24),destination=dict(x=10,y=-35)))
        for m in range(8) for h in (3,5,7)]


def movement(seat, held=False):
    return {'Scenario':dict(kind=H.MOVEMENT,payload=[0,0,0,0,int(held),0,0,seat])}


class HighLedgeTests(unittest.TestCase):
    def test_replay_changes_only_the_binary_destination_and_opt_in_probe(self):
        old=['old','--seat','0','--mode','duel','--out','old-dir','--live-claim-stopping','false']
        source={'runs':{H.A.TARGET:dict(command=old)}}
        new=H.command(source,Path('/binary'),Path('/probe'))
        self.assertEqual(new,['/binary','--seat','0','--mode','duel','--out',
                             '/probe/raw/'+H.A.TARGET,'--live-claim-stopping','false',
                             '--probe-high-ledge',H.SPEC])
        self.assertEqual(source['runs'][H.A.TARGET]['command'],old)

    def test_selection_requires_real_clearance_and_preserves_frozen_order(self):
        data=candidates()
        self.assertIsNone(H.first_candidate(data))
        data[4]['corridor_clear']=True; data[6]['corridor_clear']=True
        self.assertIs(H.first_candidate(data),data[4])
        for changed in [data[::-1],data[:-1]]:
            with self.assertRaises(AssertionError):H.first_candidate(changed)
        changed=copy.deepcopy(data);changed[0]['corridor_clear']=None
        with self.assertRaises(AssertionError):H.first_candidate(changed)
        changed=copy.deepcopy(data);changed[0]['ordinary_height_allowed']=True
        with self.assertRaises(AssertionError):H.first_candidate(changed)

    def test_fork_can_change_only_the_selected_pilots_movement(self):
        weapon={'Scenario':dict(kind=123,payload=[1,0,0])}
        original=[movement(0),weapon,movement(1)]
        H.controls_only(original,[movement(0),weapon,movement(1,True)],1)
        for altered in [[movement(0,True),weapon,movement(1,True)],
                        [movement(0),{},movement(1,True)],
                        [movement(0),weapon,movement(0,True)],original[:2]]:
            with self.assertRaises(AssertionError):H.controls_only(original,altered,1)
        with self.assertRaises(AssertionError):H.controls_only(original+[movement(1)],original+[movement(1)],1)


if __name__=='__main__':unittest.main()
