"""Acquisition audits keep sampling, selection, censoring and domains separate."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

from test_site_acquisition import row, sample as base_sample, pilot
from test_acquisition_probe import fixture as probe_fixture, acquisition, row as probe_row

SPEC = importlib.util.spec_from_file_location('acquisition_waits',
    Path(__file__).resolve().parents[1]/'audit-acquisition-waits.py')
W = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(W)


def sample(tick=0, **kwargs):
    return base_sample(tick,**dict(dict(objective_required=False),**kwargs))


def chosen(tick, **kwargs):
    return sample(tick, chosen_site=dict(planet=0,bearing=3),site_query='survey',**kwargs)


def native_row(tick=12):
    r = row(tick)
    p = pilot(r)
    p['planet']['claim'] = dict(owner=None,flag=None,claimant=None,phase='idle',progress=0)
    r['mission']['capture']['acquisition'] = dict(tick=tick,planet=0,
        revision=p['planet']['revision'],reason='candidates_rejected',checks=dict(unsafe_solar=4))
    return r


def probe():
    case,report,rows = probe_fixture()
    report['transfer_probe'].update(seat=case['seat'],destination=case['destination'])
    return report,rows


class AcquisitionWaitTests(unittest.TestCase):
    def test_handoff_excludes_choice_endpoint_and_does_not_predict_duration(self):
        samples = [sample(10,site_query='not_requested'),chosen(11)]
        result = W.summarize_window(samples,10,11,True,0,0)
        self.assertEqual(result['wait_ticks'],1)
        self.assertEqual(result['observed_wait_states'],{'scan_not_requested':1})
        self.assertTrue(result['chosen_on_first_observed_survey'])
        self.assertEqual(result['endpoint']['tick'],11)
        self.assertIsNone(result['acquisition_prediction_seconds'])

    def test_cadence_wait_is_not_terrain_rejection(self):
        samples = [sample(10,site_query='not_requested')]
        samples += [sample(t,site_query='deferred',site_count=0) for t in range(11,15)]
        samples += [chosen(15)]
        result = W.summarize_window(samples,10,15,True,0,0)
        self.assertEqual(result['deferred_ticks'],4)
        self.assertTrue(result['no_flag_four_hz_phase_agrees'])
        self.assertTrue(result['chosen_on_first_observed_survey'])
        self.assertEqual(result['native_wait_reasons'],{})
        self.assertEqual(result['claim_domains'],{'not_recorded':6})

    def test_bad_cadence_phase_and_next_tick_are_reported(self):
        for change in ('phase','next_tick'):
            s = sample(14 if change == 'next_tick' else 15,site_query='deferred',next_scan_tick=30)
            end = s['tick']+1
            result = W.summarize_window([s,chosen(end)],s['tick'],end,True,0,0)
            self.assertFalse(result['no_flag_four_hz_phase_agrees'])

    def test_enemy_flag_and_changed_objective_are_not_neutral_cadence(self):
        for first,last,domain in [(True,True,'enemy_flag'),(True,False,'changed')]:
            result = W.summarize_window([sample(14,site_query='deferred',objective_required=first),
                chosen(15,objective_required=last)],14,15,True,0,0)
            self.assertEqual(result['objective_domain'],domain)
            self.assertIsNone(result['no_flag_four_hz_phase_agrees'])

    def test_measured_empty_scan_and_later_choice_remain_distinct(self):
        result = W.summarize_window([sample(10,site_query='survey',site_count=0),chosen(11)],10,11,True,0,0)
        self.assertEqual(result['observed_wait_states'],{'measured_scan_empty':1})
        self.assertFalse(result['chosen_on_first_observed_survey'])

    def test_censor_is_not_a_choice_and_has_no_invented_endpoint(self):
        result = W.summarize_window([sample(10),sample(11)],10,12,False,0,0)
        self.assertEqual(result['wait_ticks'],2)
        self.assertFalse(result['chosen'])
        self.assertIsNone(result['endpoint'])
        self.assertIsNone(result['chosen_on_first_observed_survey'])

    def test_gaps_duplicates_and_wrong_actors_cannot_yield_timing(self):
        for samples in ([sample(10),chosen(12)],[sample(10),sample(10),chosen(11)],
                        [sample(10,seat=1),chosen(11)]):
            with self.assertRaises(ValueError):
                W.summarize_window(samples,10,11,True,0,0)

    def test_wrong_missing_or_earlier_site_cannot_become_endpoint(self):
        for samples in ([sample(10),sample(11)],[chosen(10),chosen(11)],
                        [sample(10),chosen(11,local_planet=1)]):
            with self.assertRaises(ValueError):
                W.summarize_window(samples,10,11,True,0,0)

    def test_stale_or_wrong_frame_native_reason_is_not_counted(self):
        for field in ('tick','planet','revision'):
            r = native_row()
            r['mission']['capture']['acquisition'][field] += 1
            sample0 = W.native_sample(r,0)
            self.assertIsNone(sample0['native'])
            self.assertEqual(sample0['native_status'],'stale')
            result = W.summarize_window([sample0],12,13,False,0,0)
            self.assertEqual(result['native_wait_reasons'],{})

    def test_current_native_rejections_are_separate_from_observation_bins(self):
        s = W.native_sample(native_row(),0)
        result = W.summarize_window([s],12,13,False,0,0)
        self.assertEqual(result['native_wait_reasons'],{'candidates_rejected':1})
        self.assertEqual(result['native_wait_checks'],{'unsafe_solar':4})
        self.assertEqual(result['observed_wait_states'],{'candidates_without_flag_requirement':1})

    def test_native_read_is_nonmutating_and_preserves_deferred_target(self):
        r = native_row()
        pilot(r)['site_query'] = {'deferred':{'next_tick':15}}
        before = copy.deepcopy(r)
        self.assertEqual(W.native_sample(r,0)['next_scan_tick'],15)
        self.assertEqual(r,before)
        for invalid in (12,11,True,12.5):
            pilot(r)['site_query']['deferred']['next_tick'] = invalid
            with self.assertRaises(ValueError): W.native_sample(r,0)

    def test_no_enemy_flag_does_not_imply_neutral_idle(self):
        r = native_row()
        self.assertEqual(W.native_sample(r,0)['claim_domain'],'neutral_idle')
        pilot(r)['planet']['claim']['owner'] = pilot(r)['owner']
        s = W.native_sample(r,0)
        self.assertFalse(s['objective_required'])
        self.assertEqual(s['claim_domain'],'other_claim')

    def test_grouping_keeps_no_arrival_and_no_choice_in_denominators(self):
        success = W.summarize_window([sample(10),chosen(11)],10,11,True,0,0)
        censored = W.summarize_window([sample(10)],10,11,False,0,0)
        groups = W.aggregate([dict(window=success),dict(window=censored),dict(window=None)])
        self.assertEqual(groups['no_enemy_flag']['attempts'],2)
        self.assertEqual(groups['no_enemy_flag']['choices'],1)
        self.assertEqual(groups['not_arrived']['attempts'],1)

    def test_input_hash_mismatch_is_fatal(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory)/'data'
            p.write_text('data')
            good = W.digest(p)
            self.assertEqual(W.bound(p,good),good)
            p.write_text('different')
            with self.assertRaises(ValueError): W.bound(p,good)

    def test_probe_binds_handoff_to_transfer_and_event_even_if_clock_is_self_consistent(self):
        report,rows = probe()
        waiting,stop = W.probe_window(report,rows,0)
        self.assertEqual([r['tick'] for r in waiting],[10])
        self.assertEqual(stop['tick'],11)
        for retime_transfer in (False,True):
            changed = copy.deepcopy(report)
            a = changed['transfer_probe']['acquisition']
            a.update(started_tick=11,observed_rows=1)
            a['outcome']['elapsed_ticks'] = 0
            if retime_transfer: changed['transfer_probe']['outcome']['tick'] = 11
            with self.assertRaises(ValueError): W.probe_window(changed,rows[1:],0)

    def test_observed_interruption_needs_its_own_stop_witness(self):
        report,rows = probe()
        report['transfer_probe']['acquisition']['outcome'].update(reason='capture_ended',site=None)
        with self.assertRaises(ValueError): W.probe_window(report,rows,0)
        rows[-1]['mission']['capture'] = None
        waiting,stop = W.probe_window(report,rows,0)
        self.assertEqual([r['tick'] for r in waiting],[10])
        self.assertIsNone(stop['mission']['capture'])
        with self.assertRaises(ValueError): W.probe_window(report,rows[:1],0)

    def test_unobserved_censor_has_no_fabricated_controller_endpoint(self):
        rows = [probe_row(10)]
        report = acquisition(rows,'runner_ended',observed=False)
        report['transfer_probe'].update(seat=0,destination=1)
        waiting,stop = W.probe_window(report,rows,0)
        self.assertEqual(waiting,rows)
        self.assertIsNone(stop)
        report['transfer_probe']['acquisition']['outcome']['reason'] = 'site_selected'
        with self.assertRaises(ValueError): W.probe_window(report,rows,0)

    def test_earlier_native_endpoint_cannot_be_hidden_by_later_censor(self):
        rows = [probe_row(10),probe_row(11),probe_row(12)]
        rows[1]['mission']['goal'] = 'recover'
        rows[-1]['mission']['capture'] = None
        report = acquisition(rows,'capture_ended')
        report['transfer_probe'].update(seat=0,destination=1)
        with self.assertRaises(ValueError): W.probe_window(report,rows,0)

    def test_native_choice_from_an_older_capture_is_not_current_acquisition(self):
        report,rows = probe()
        rows[-1]['mission']['capture']['started_tick'] = 9
        with self.assertRaises(ValueError): W.probe_window(report,rows,0)

    def test_probe_counts_must_match_dense_controller_rows(self):
        for field in ('observed_rows','last_observed_tick','initial_replans'):
            report,rows = probe()
            report['transfer_probe']['acquisition'][field] += 1
            with self.assertRaises(ValueError): W.probe_window(report,rows,0)

    def test_zero_wait_interruption_is_not_success_or_observed_domain(self):
        result = W.summarize_window([],10,10,False,0,0)
        self.assertFalse(result['chosen'])
        self.assertEqual(result['objective_domain'],'not_observed')
        self.assertIsNone(result['acquisition_prediction_seconds'])
