import copy
import importlib.util
from pathlib import Path
import unittest

from test_powered_mission import publication

spec=importlib.util.spec_from_file_location('focused',Path(__file__).parents[1]/'validate-focused-routes.py')
M=importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class FocusedRoutesTests(unittest.TestCase):
    def test_frozen_plan_keeps_both_models_and_every_shared_case(self):
        items=M.plan()
        self.assertEqual(len(items),14)
        self.assertEqual(sum(i['kind']=='directed'for i in items),6)
        self.assertEqual({i['seat']for i in items},{0,1})
        for item in items:
            self.assertEqual(M.arguments(item,True),M.arguments(item,False)+['--focused-objective-routes','true'])
            self.assertEqual(item['delivery'],'shared')

    def test_partial_delivery_needs_positive_routes_and_original_age(self):
        row=publication();row.update(seat=0,mission=dict(powered_capture=True),capture=dict(site=dict(planet=1,bearing=0)))
        row['objective_evidence']['generation']=4
        survey=row['landing_objective'];survey.update(validated_routes_only=True,actual=None)
        good=dict(site=row['capture']['site'],endpoint=dict(id=1),outbound=dict(failure=None),returning=dict(failure=None))
        survey['sites']=[good]
        self.assertEqual(M.publication(row)['age'],10)
        self.assertTrue(M.publication(row)['matching_capture_site'])
        for key,value in [('endpoint',None),('returning',None),('outbound',dict(failure='disconnected'))]:
            bad=copy.deepcopy(row);bad['landing_objective']['sites'][0][key]=value
            with self.assertRaises(AssertionError):M.publication(bad)
        survey['sites']=[]
        with self.assertRaises(AssertionError):M.publication(row)


if __name__=='__main__':unittest.main()
