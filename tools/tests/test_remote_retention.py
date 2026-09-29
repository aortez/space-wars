import copy
import importlib.util
from pathlib import Path
import unittest


def module(name,path):
    spec = importlib.util.spec_from_file_location(name,path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


R = module('retention',Path(__file__).parents[1]/'retain-remote-surveys.py')
S = module('screen_fixture',Path(__file__).with_name('test_remote_arrival_screen.py'))


def fixture():
    _,source,_,row,history = S.fixture()
    planet = row['observation']['planets'][0]
    planet['claim'].update(planet=0,stage_required_seconds=3.,flag_interaction_range=4.,captures=0,neutralizations=0)
    p = row['observation']['local']['combat']['recovery']['flight']['pilot']
    p.update(owner='player_1',vehicle=0,spaceling=1,ship_available=True,ship_form='ship')
    source['retained_source'] = dict(actor='player_1',vehicle=0,spaceling=1,episode_seed=42,tick=11,
        groups=[dict(identity=R.neutral_identity(planet),observed_tick=11,survey=source['remote_source'])])
    source['remote_source']['candidates'][0]['reason'] = 'physics advanced; remeasurement required'
    return source,{(0,t):copy.deepcopy(row) for t in [10,11]},history,42


class RetainedSurveyAuditTests(unittest.TestCase):
    def test_original_frame_dispatch_and_current_identity_are_required(self):
        self.assertEqual(R.audit_memory(*fixture()),dict(groups=1,measurements=1,**{'finding:measured':1}))

    def test_mutations_cannot_rebase_samples_or_hide_lost_identity(self):
        for change in ['frame','revision','ownership','pilot','future','generation','witness','capacity','clock','epoch','duplicates','status','reason']:
            with self.subTest(change=change):
                source,rows,history,seed = fixture()
                memory = source['retained_source']
                group = memory['groups'][0]
                if change == 'frame': rows[0,10]['observation']['planets'][0]['motion']['position']['x'] += 1
                elif change == 'revision': rows[0,10]['observation']['planets'][0]['revision'] += 1
                elif change == 'ownership': rows[0,10]['observation']['planets'][0]['claim']['owner'] = 'player_2'
                elif change == 'pilot': rows[0,10]['observation']['local']['combat']['recovery']['flight']['pilot']['spaceling'] += 1
                elif change == 'future': group['survey']['candidates'][0]['measurement']['tick'] = 11
                elif change == 'generation': group['survey']['generation'] = 11
                elif change == 'witness': history.clear()
                elif change == 'capacity': memory['groups'] *= 3
                elif change == 'clock': group['observed_tick'] = 10
                elif change == 'epoch': memory['episode_seed'] += 1
                elif change == 'status': group['survey']['candidates'][0]['status'] = 'measured'
                elif change == 'reason': group['survey']['candidates'][0]['reason'] = None
                else: group['survey']['candidates'] *= 2
                with self.assertRaises(AssertionError): R.audit_memory(source,rows,history,seed)

    def test_fresh_negative_and_new_generation_cannot_be_hidden_by_old_positive(self):
        for newer_generation in [False,True]:
            source,rows,history,seed = fixture()
            source['source_tick'] = source['retained_source']['tick'] = 12
            rows[0,12] = copy.deepcopy(rows[0,11])
            newer = copy.deepcopy(history[0])
            newer['tick'] = newer['evidence']['candidates'][0]['measurement']['tick'] = 11
            newer['evidence']['candidates'][0]['measurement']['finding'] = 'no_landing'
            if newer_generation: newer['evidence']['generation'] = 11
            history.append(newer)
            with self.assertRaisesRegex(AssertionError,'newer dispatched'):
                R.audit_memory(source,rows,history,seed)


if __name__ == '__main__': unittest.main()
