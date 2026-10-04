import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('selection_analysis',Path(__file__).parents[1]/'analyze-projectile-selection.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def record(winner='player_1',choice=None,warning=False):
    return dict(round=dict(outcome=dict(winner=winner),pilots=[dict(death_tick=None),dict(death_tick=None)]),
                physical=dict(claimed=2,departed=1),final_recovery=dict(ships_lost=1,rebuilds=1),
                selection=choice,attempt=dict(started_tick=10) if warning else None,parity=dict(exact=True))


class SelectionAnalysisTests(unittest.TestCase):
    def test_seat_aware_wins_and_abstentions_stay_in_separate_cohorts(self):
        cases=[dict(key=k,group=g,scope='transfer',item=dict(seat=s,seed=42))
               for k,g,s in [('a','historical',0),('b','heldout_asteroids',1)]]
        refs={'a':record(),'b':record()}
        runs={'a-guarded_brake':record(choice=dict(action='observe',reason='short'),warning=True),
              'b-guarded_brake':record(winner='player_2')}
        groups,paired=A.compare(cases,runs,refs)
        self.assertEqual(groups['historical']['abstained_cases'],1)
        self.assertEqual(groups['historical']['brake_cases'],0)
        self.assertEqual(groups['heldout_asteroids']['control']['wins'],0)
        self.assertEqual(groups['heldout_asteroids']['selector']['wins'],1)
        self.assertEqual(paired['b']['delta']['wins'],1)
        self.assertEqual(groups['heldout_asteroids']['no_warning_cases'],1)


if __name__=='__main__':unittest.main()
