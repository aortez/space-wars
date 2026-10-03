import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('analysis',Path(__file__).parents[1]/'analyze-projectile-response.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def damage(contact_tick=8, damage_tick=8, source='cannon', spawn=1, lost=False):
    return dict(last_contact_tick=contact_tick,last_contact_source='cannon',last_contact_spawn_tick=spawn,
                last_damage_tick=damage_tick,last_source=source,last_damage_percent=2.,last_ship_lost=lost)


class ResponseAnalysisTests(unittest.TestCase):
    def test_simultaneous_launches_cannot_be_assigned_to_one_projectile(self):
        contacts=[dict(tick=20,source='cannon',spawn_tick=10)]
        self.assertEqual(A.attributed_contacts(contacts,10,{1},1),contacts)
        self.assertIsNone(A.attributed_contacts(contacts,10,{1,2},1))

    def test_destruction_on_foot_is_a_loss_even_when_form_and_vehicle_do_not_change(self):
        def pilot(tick,count):
            return dict(tick=tick,vehicle=0,ship_form='ship',location='on_foot',recovery=dict(ships_lost=count))
        observations=[pilot(10,0),pilot(11,1),pilot(12,1),pilot(13,2),pilot(14,2)]
        losses=A.ship_losses(observations,{11:damage(lost=True),13:damage(lost=True)})
        self.assertEqual([r['tick'] for r in losses],[11,13])
        self.assertEqual(losses[0]['previous_pilot'],observations[0])

    def test_sticky_damage_and_contact_counters_are_not_repeated_as_new_events(self):
        changes=[dict(damage=damage()),dict(damage=damage(12,12)),dict(damage=damage(12,13,'laser')),
                 dict(damage=damage(12,13,'laser'))]
        contacts,hits=A.receipts(changes,damage(12,13,'laser'),10)
        self.assertEqual([c['tick'] for c in contacts],[12])
        self.assertEqual([(h['tick'],h['source']) for h in hits],[(12,'cannon'),(13,'laser')])

    def test_final_step_contact_and_damage_do_not_need_another_pre_step_sample(self):
        contacts,hits=A.receipts([dict(damage=damage(None,None))],damage(20,20,spawn=19,lost=True),10)
        self.assertEqual(contacts,[dict(tick=20,source='cannon',spawn_tick=19)])
        self.assertEqual(hits,[dict(tick=20,source='cannon',amount=2.,ship_lost=True)])


if __name__=='__main__':unittest.main()
