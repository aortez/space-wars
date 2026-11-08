"""The extended run may change its update budget, never its reproduced state."""
import copy
import unittest
import torch
from tools.flight_audit.extended_hold import verify_parent32


class ExtendedHoldTests(unittest.TestCase):
    def pair(self):
        old={'update':32,'contract':{'config':{'updates':32,'entropy_weight':.01},'bridge_sha256':'fixed'},
             'model':{'weight':torch.tensor([1.,2.])},'optimizer':{'step':32},
             'collector':{'cursors':[1,2,3,4]},'torch_rng':torch.tensor([9,8]),'history':[{'update':32}]}
        new=copy.deepcopy(old);new['contract']['config']['updates']=128
        return new,old

    def test_only_declared_cap_is_normalized_without_mutation(self):
        new,old=self.pair();verify_parent32(new,old)
        self.assertEqual(new['contract']['config']['updates'],128)
        self.assertEqual(old['contract']['config']['updates'],32)

    def test_model_optimizer_collector_rng_and_history_changes_fail(self):
        for key,change in [('model',{'weight':torch.tensor([1.,3.])}),('optimizer',{'step':31}),
                           ('collector',{'cursors':[1,2,3,5]}),('torch_rng',torch.tensor([9,9])),('history',[])]:
            new,old=self.pair();new[key]=change
            with self.assertRaises(ValueError):verify_parent32(new,old)

    def test_other_contract_changes_fail(self):
        new,old=self.pair();new['contract']['config']['entropy_weight']=0.
        with self.assertRaises(ValueError):verify_parent32(new,old)
        new,old=self.pair();new['contract']['bridge_sha256']='changed'
        with self.assertRaises(ValueError):verify_parent32(new,old)

    def test_wrong_checkpoint_or_budget_fails(self):
        for which,key,value in [('current','update',31),('parent','update',31),('current','cap',256),('parent','cap',128)]:
            new,old=self.pair();state=new if which=='current' else old
            if key=='cap':state['contract']['config']['updates']=value
            else:state[key]=value
            with self.assertRaises(ValueError):verify_parent32(new,old)


if __name__=='__main__':unittest.main()
