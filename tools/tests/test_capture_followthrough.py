import copy
import importlib.util
import json
import tempfile
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('followthrough', Path(__file__).resolve().parents[1]/'probe-capture-followthrough.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


def fixture():
    planet = dict(index=1, revision=1, radius=60, motion=dict(position=dict(x=0,y=0), angle=0),
        claim=dict(owner=None,flag=None,claimant=None,phase='idle',progress=0,stage_required_seconds=3,
                   flag_interaction_range=3,captures=0))
    site = dict(planet=1,bearing=2)
    p = dict(tick=100,owner='player_1',vehicle=0,spaceling=0,location=dict(aboard=0),ship_available=True,
        ship_health=100,ship_form='ship',actor=None,ship=dict(position=dict(x=0,y=80)),planet=planet,
        landing=dict(phase='flying'),last_transfer='ready',transfers=0)
    c = dict.fromkeys(P.RETRIES, 0)
    c.update(started_tick=100,site=site,failed_tick=None,failure=None,completed_tick=None,
        landing=dict(site=None,invalidations=0,landing_retries=0,landed_tick=None,claimed_tick=None,boarded_tick=None,
                     blocked_reason=None,touchdown_adjustments=0))
    m = dict(capture=c,recovery=None,goal='capture',target=1,events=[],completed_sorties=0,replans=0)
    row = dict(tick=100,seat=0,mission=m,observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=p)))),planets=[planet]))
    source = dict(pilot=copy.deepcopy(p),mission=copy.deepcopy(m))
    return source, [row]


def next_row(rows):
    row = copy.deepcopy(rows[-1])
    row['tick'] += 1
    P.P.pilot(row)['tick'] = row['tick']
    rows.append(row)
    return P.P.pilot(row), row['mission']


def complete_rows(tactical=False):
    source, rows = fixture()
    p,m = next_row(rows)
    p['landing']['phase'] = 'landed'
    m['capture']['landing']['landed_tick'] = p['tick']
    p,m = next_row(rows)
    p.update(location='on_foot',actor=dict(position=dict(x=0,y=60)),last_transfer='exited',transfers=1)
    p,m = next_row(rows)
    p['planet']['claim'].update(claimant='player_1',phase='raising',progress=.1)
    p,m = next_row(rows)
    p['planet']['claim'].update(owner='player_1',flag=dict(player='player_1'),captures=1)
    m['capture']['landing']['claimed_tick'] = p['tick']
    p,m = next_row(rows)
    p.update(location=dict(aboard=0),last_transfer='boarded',transfers=2)
    m['capture']['landing']['boarded_tick'] = p['tick']
    if tactical:
        p,m = next_row(rows)
        m['capture']['completed_tick'] = p['tick']
    p,m = next_row(rows)
    if not tactical: p['ship']['position']['y'] = 131
    m.update(target=None,capture=None,goal='select',completed_sorties=1)
    m['events'].append(dict(tick=p['tick'],planet=1,kind='departed'))
    return source, rows


class FollowthroughAudit(unittest.TestCase):
    def test_full_trace_keeps_both_players_and_excludes_only_final_command(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            case = dict(seat=0,source_tick=0)
            report = dict(elapsed_ticks=1,transfer_probe=dict(capture_followthrough=dict(outcome=dict(reason='capture_replanned',controller_observed=True))))
            rows = []
            for tick in range(2):
                for seat in range(2):
                    _,fixture_rows = fixture()
                    row = fixture_rows[0]
                    row.update(tick=tick,seat=seat)
                    P.P.pilot(row)['tick'] = tick
                    rows.append(row)
            sensors = [dict(tick=r['tick']+1,seat=r['seat']) for r in rows]
            def write(r,s):
                for name,items in [('trace.jsonl',r),('sensors.jsonl',s)]:
                    (root/name).write_text(''.join(json.dumps(v)+'\n' for v in items))
            write(rows,sensors)
            self.assertEqual(P.read_control(case,root,report)[1]['controller_rows'],4)
            for mutation in ['opponent_clock','sensor_clock','missing_endpoint']:
                r,s = copy.deepcopy(rows),copy.deepcopy(sensors)
                if mutation == 'opponent_clock': r[-1]['tick'] += 1
                elif mutation == 'sensor_clock': s[-1]['tick'] += 1
                else: r = r[:-2]
                write(r,s)
                with self.assertRaises(AssertionError): P.read_control(case,root,report)
            report['transfer_probe']['capture_followthrough']['outcome'] = dict(reason='match_finished',controller_observed=False)
            write(rows[:-2],sensors[:-2])
            self.assertEqual(P.read_control(case,root,report)[1]['controller_rows'],2)

    def test_suffix_flag_and_shadow_charges_are_reconciled(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root/'live-planning.csv').write_text('tick,graph,queries,total_graph,total_queries,graph_budget,query_budget\n')
            flag = [dict(tick=t,remaining_after_evaluation=dict(graph=3,physics_queries=384),
                         allocation=dict(tick=t+1,allowance=dict(graph=2,physics_queries=384),
                            charged=dict(graph=1,physics_queries=0),jobs=[dict(charged=dict(graph=1,physics_queries=0),
                                limits=dict(per_tick=dict(graph=2,physics_queries=384)))])) for t in range(2)]
            shadow = [dict(tick=t,remaining_after_flag_survey=dict(graph=2,physics_queries=0),
                           charged=dict(graph=1,physics_queries=0)) for t in range(2)]
            report = dict(elapsed_ticks=2,live_objective_planning=dict(telemetry=dict(graph=0,physics_queries=0)),
                          mission_evaluation=dict(charged=2),flag_survey=dict(telemetry=dict(graph=2,physics_queries=0),shadow=dict(charged=2)))
            def write(f,s):
                for name,rows in [('flag-survey-work.jsonl',f),('flag-value-shadow-work.jsonl',s)]:
                    (root/name).write_text(''.join(json.dumps(r)+'\n' for r in rows))
            write(flag,shadow)
            self.assertEqual(P.audit_budget(root,report)['max_combined_graph'],3)
            for mutation in ['flag','shadow','job','residual']:
                f,s = copy.deepcopy(flag),copy.deepcopy(shadow)
                # Prefix tick0 remains unchanged; corrupt only suffix tick1.
                if mutation == 'flag': f[1]['allocation']['charged']['graph'] = 3
                elif mutation == 'shadow': s[1]['charged']['graph'] = 3
                elif mutation == 'job': f[1]['allocation']['jobs'][0]['charged']['graph'] = 0
                else: s[1]['remaining_after_flag_survey']['graph'] = 3
                write(f,s)
                with self.assertRaises(AssertionError): P.audit_budget(root,report)

    def test_both_native_departure_paths_after_cleared_task(self):
        for tactical in [False,True]:
            source, rows = complete_rows(tactical)
            clocks, completed, touchdown, reason = P.reconstruct(source,rows)
            self.assertEqual(reason, 'departed')
            self.assertEqual(list(clocks.values()), [100,101,102,103,104,105,107 if tactical else 106])
            self.assertEqual(completed, 106 if tactical else None)
            self.assertEqual(touchdown['tick'],101)

    def test_native_clocks_cannot_replace_physical_witnesses(self):
        for phase, field, value in [('landed','phase','flying'),('exited','last_transfer','ready'),
                                   ('claimed','captures',0),('boarded','last_transfer','ready')]:
            source, rows = complete_rows()
            index = dict(landed=1,exited=2,claimed=4,boarded=5)[phase]
            rows = rows[:index+1]
            p = P.P.pilot(rows[-1])
            if phase == 'landed': p['landing'][field] = value
            elif phase == 'claimed': p['planet']['claim'][field] = value
            else: p[field] = value
            clocks,_,_,reason = P.reconstruct(source,rows)
            self.assertEqual(reason, 'unwitnessed_milestone')
            self.assertIsNone(clocks[phase])

    def test_retry_and_identity_change_win_over_progress(self):
        for kind in ['inner_retry','outer_retry','revision','foreign_owner','recovery','ship_lost']:
            source,rows = complete_rows()
            rows = rows[:2]
            p,m = P.P.pilot(rows[-1]),rows[-1]['mission']
            if kind == 'inner_retry': m['capture']['landing']['landing_retries'] += 1
            elif kind == 'outer_retry': m['capture']['solar_replans'] += 1
            elif kind == 'revision': p['planet']['revision'] += 1
            elif kind == 'foreign_owner': p['planet']['claim']['owner'] = 'player_2'
            elif kind == 'recovery': m['goal'] = 'recover'
            else: p['ship_health'] = 0
            clocks,_,_,reason = P.reconstruct(source,rows)
            expected = dict(inner_retry='capture_replanned',outer_retry='capture_replanned',revision='material_or_rules_changed',
                            foreign_owner='foreign_claim_context',recovery='recovery',ship_lost='ship_or_pilot_lost')[kind]
            self.assertEqual(reason,expected)
            self.assertIsNone(clocks['landed'])

    def test_sun_avoidance_advisory_hatch_and_damage_keep_same_attempt(self):
        source, rows = fixture()
        p,m = next_row(rows)
        m['goal'] = 'avoid_sun'
        m['capture']['landing'].update(blocked_reason='no usable hatch floor',touchdown_adjustments=1,
                                       site=m['capture']['site'])
        p['ship_health'] = 98
        self.assertIsNone(P.reconstruct(source,rows)[-1])

    def test_frame_change_only_after_boarding_and_ownership_remains_required(self):
        for after_board in [False,True]:
            source,rows = complete_rows()
            rows = rows[:-1] if after_board else rows[:2]
            p,m = next_row(rows)
            p['planet'] = dict(index=2)
            clocks,_,_,reason = P.reconstruct(source,rows)
            self.assertEqual(reason,None if after_board else 'approach_frame_changed')
        source,rows = complete_rows()
        rows = rows[:-1]
        p,m = next_row(rows)
        p['planet']['claim'].update(owner=None,flag=None)
        self.assertEqual(P.reconstruct(source,rows)[-1], 'ownership_lost')

    def test_valid_landing_is_retained_before_invalid_later_clock(self):
        source, rows = complete_rows()
        rows = rows[:2]
        rows[-1]['mission']['capture']['landing']['claimed_tick'] = 101
        clocks,_,touchdown,reason = P.reconstruct(source,rows)
        self.assertEqual(reason,'unwitnessed_milestone')
        self.assertEqual(clocks['landed'],101)
        self.assertIsNone(clocks['claimed'])
        self.assertEqual(touchdown['tick'],101)

    def test_loss_beats_same_tick_departure(self):
        source,rows = complete_rows()
        P.P.pilot(rows[-1])['ship_health'] = 0
        clocks,_,_,reason = P.reconstruct(source,rows)
        self.assertEqual(reason,'ship_or_pilot_lost')
        self.assertIsNone(clocks['departed'])

    def test_dense_clock_required_and_no_progress_from_finish(self):
        source,rows = fixture()
        next_row(rows)
        rows[-1]['tick'] += 1
        with self.assertRaises(AssertionError): P.reconstruct(source,rows)
        clocks = dict.fromkeys(P.NAMES) | dict(choice=100)
        prediction = dict.fromkeys(P.PHASES,1)
        phases = P.phases(clocks,prediction,dict(tick=200,reason='match_finished'))
        self.assertEqual(phases['landing']['status'],'censored')
        self.assertIsNone(phases['landing']['observed_seconds'])
        self.assertIsNone(phases['landing']['error_seconds'])
        self.assertEqual(phases['exit']['status'],'not_started')

    def test_phase_errors_only_for_completed_intervals_even_with_unknown_prediction(self):
        clocks = dict.fromkeys(P.NAMES) | dict(choice=100,landed=220,exited=221)
        result = P.phases(clocks,dict.fromkeys(P.PHASES,1),dict(tick=250,reason='capture_replanned'))
        self.assertEqual(result['landing']['error_seconds'],1)
        self.assertEqual(result['outbound']['status'],'interrupted')
        self.assertIsNone(result['outbound']['error_seconds'])
        unknown = P.phases(clocks,None,dict(tick=250,reason='capture_replanned'))
        self.assertEqual(unknown['landing']['observed_seconds'],2)
        self.assertTrue(all(v['error_seconds'] is None for v in unknown.values()))

    def test_horizon_is_from_choice_and_inclusive(self):
        source,rows = fixture()
        for _ in range(P.HORIZON_SECONDS*60): next_row(rows)
        clocks,_,_,reason = P.reconstruct(source,rows)
        self.assertEqual(reason,'observation_horizon')
        self.assertEqual(clocks,dict.fromkeys(P.NAMES) | dict(choice=100))


if __name__ == '__main__': unittest.main()
