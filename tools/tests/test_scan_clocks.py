"""Scan schedules preserve history and do not become selection-time estimates."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from test_site_acquisition import row as acquisition_row

SPEC = importlib.util.spec_from_file_location('scan_clocks', Path(__file__).resolve().parents[1]/'compare-scan-clocks.py')
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)


def fixture():
    planet = dict(index=1, revision=3, claim=dict(planet=1, owner=None, flag=None,
        claimant=None, phase='idle', progress=0, stage_required_seconds=3, flag_interaction_range=2.8))
    p = dict(owner='player_1', tick=10, ship_form='ship', planet=planet,
        queries_ready=True, site_query='not_requested')
    row = dict(tick=10, seat=0, observation=dict(planets=[planet],
        local=dict(combat=dict(recovery=dict(flight=dict(pilot=p))))))
    clock = dict(model='conditional_neutral_scan_v1', source_tick=10, actor='player_1',
        destination=1, revision=3, form='ship', cadence='FourHz', last_survey=None,
        complete=True, handoff_tick=30, request_tick=31, opportunity_tick=31,
        handoff_to_scan_ticks=1, unknown=None, site_selection_seconds=None, match_remaining_ticks=None)
    forecast = dict(source_tick=10, ticks=20, destination=1, end='kinematic_handoff', scan_clock=clock)
    native = dict(seat=0, destination=1, first_scan=33, handoff=32, choice=35,
        last_survey=None, neutral_material=True, revisions=[3], forms=['ship'],
        source_continuity={'0:10': True}, uninterrupted_requests=True)
    return forecast, row, native


class ScanClockTests(unittest.TestCase):
    def test_conditional_sum_binds_scan_wait_and_original_geometry(self):
        source = dict(site={'planet':1, 'bearing':3}, measurement_tick=10, arrival_tick=30,
            eligible_sides=[1], conditional_seconds=20.0, phases={'landing':17.0, 'claim':3.0},
            projected={'id':{'planet':1, 'bearing':3}}, unknown=None)
        f, _, _ = fixture()
        f['handoff_seconds'] = 20/60
        candidate = dict(destination=1, forecast=f, remote_arrival=dict(complete=True,
            unknown=None, local_reference=dict(complete=True, unknown=None, references=[source])))
        composition = dict(model='conditional_first_scan_success_v1', actor='player_1',
            source_tick=10, destination=1, revision=3, handoff_tick=30, scan_tick=31,
            remaining_trip_seconds=None, conditions='first scan selects this retained usable site',
            travel_seconds=20/60, scan_wait_seconds=1/60, unknown=None,
            references=[dict(site=source['site'], measurement_tick=10, geometry_tick=30,
                eligible_sides=[1], local_seconds=20.0, conditional_total_seconds=20+21/60,
                unknown=None, exceeds_match_time=None)])
        S.audit_composition(candidate, composition)
        for mutation in range(8):
            bad = copy.deepcopy(composition)
            r = bad['references'][0]
            if mutation == 0: bad['scan_wait_seconds'] = 0
            elif mutation == 1: r['geometry_tick'] = 31
            elif mutation == 2: r['measurement_tick'] = 11
            elif mutation == 3: r['conditional_total_seconds'] -= 1/60
            elif mutation == 4: bad['remaining_trip_seconds'] = 20+21/60
            elif mutation == 5: r['eligible_sides'] = [-1]
            elif mutation == 6: bad['unknown'] = 'hide valid result'
            else: r['conditional_total_seconds'] = None
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                S.audit_composition(candidate, bad)
        candidate['remote_arrival']['local_reference']['references'][0]['unknown'] = 'native site unavailable'
        with self.assertRaises(AssertionError): S.audit_composition(candidate, composition)
        composition['references'][0].update(conditional_total_seconds=None, unknown='native site unavailable')
        S.audit_composition(candidate, composition)

    def test_arrival_error_is_separate_from_schedule_and_choice(self):
        f, row, native = fixture()
        r = S.audit_clock(f, 0, row, None, native)
        self.assertEqual(r['native']['handoff_error_ticks'], -2)
        self.assertEqual(r['native']['scan_error_ticks'], -2)
        self.assertTrue(r['native']['actual_handoff_schedule_matches'])
        self.assertEqual(r['native']['native_choice_tick'], 35)
        self.assertIsNone(r['clock']['site_selection_seconds'])

    def test_both_seats_and_all_retained_phases(self):
        last = dict(tick=1, planet=1, form='ship')
        for seat in range(2):
            for handoff in range(30, 45):
                tick = S.scan_tick(handoff, seat, 1, 'ship', last, 'FourHz')
                self.assertTrue(1 <= tick-handoff <= 15)
                self.assertEqual((tick+seat*7) % 15, 0)
                for changed in [None, dict(last, planet=0), dict(last, form='escape_pod')]:
                    self.assertEqual(S.scan_tick(handoff, seat, 1, 'ship', changed, 'FourHz'), handoff+1)
                self.assertEqual(S.scan_tick(handoff, seat, 1, 'ship', last, 'EveryTick'), handoff+1)

    def test_wrong_identity_epoch_history_or_invented_choice_time_fails(self):
        for key, value in dict(source_tick=9, actor='player_2', revision=4, destination=2,
            last_survey=dict(tick=1, planet=1, form='ship'), opportunity_tick=32,
            handoff_tick=29, site_selection_seconds=0, complete=False).items():
            f, row, native = fixture()
            f['scan_clock'][key] = value
            with self.subTest(key=key), self.assertRaises((ValueError, AssertionError)):
                S.audit_clock(f, 0, row, None, native)

    def test_native_material_and_history_changes_withhold_comparison(self):
        for key, value in dict(last_survey=dict(tick=20, planet=1, form='ship'),
            neutral_material=False, revisions=[3, 4], forms=['escape_pod'], first_scan=None,
            source_continuity={'0:10': False}, uninterrupted_requests=False).items():
            f, row, native = fixture()
            native[key] = value
            r = S.audit_clock(f, 0, row, None, native)
            self.assertFalse(r['native']['comparable'])
            self.assertNotIn('scan_error_ticks', r['native'])

    def test_hypothetical_destination_gets_no_native_outcome(self):
        f, row, native = fixture()
        native['destination'] = 2
        self.assertIsNone(S.audit_clock(f, 0, row, None, native)['native'])

    def test_pending_nonarrival_and_nonneutral_are_retained(self):
        for kind in ['pending', 'horizon', 'enemy']:
            f, row, native = fixture()
            c = f['scan_clock']
            c.update(request_tick=None, opportunity_tick=None, handoff_to_scan_ticks=None)
            if kind == 'pending':
                f['end'] = None
                c.update(complete=False, handoff_tick=None)
            elif kind == 'horizon':
                f['end'] = 'horizon'
                c.update(handoff_tick=None, unknown='no conditional handoff')
            else:
                row['observation']['planets'][0]['claim']['owner'] = 'player_2'
                c['unknown'] = 'claim outside neutral-idle scan domain'
            result = S.audit_clock(f, 0, row, None, native)
            self.assertFalse(result['native']['comparable'])
            self.assertIsNone(result['clock']['opportunity_tick'])

    def test_native_duration_boundary_uses_integer_nanoseconds(self):
        for nanos in [21*16_666_667-1, 21*16_666_667, 21*16_666_667+1]:
            f, row, native = fixture()
            c = f['scan_clock']
            row['observation']['match_context'] = dict(remaining_seconds=nanos/1e9)
            c['match_remaining_ticks'] = (nanos+16_666_666)//16_666_667
            if c['match_remaining_ticks'] == 21:
                c.update(opportunity_tick=None, handoff_to_scan_ticks=None, unknown='match ends before scan opportunity')
            S.audit_clock(f, 0, row, None, native)

    def test_native_prefix_reconstructs_only_ready_survey_history(self):
        _, row, _ = fixture()
        rows = []
        for tick in range(3):
            r = copy.deepcopy(row)
            r['tick'] = tick
            p = S.W.P.pilot(r)
            p['tick'] = tick
            p['site_query'] = 'survey'
            p['queries_ready'] = tick == 0
            rows.append(r)
        report = dict(transfer_comparison=dict(sources=[dict(seat=0, source_tick=2, last_snapshot={})]))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def write(items):
                with gzip.open(root/'trace.jsonl.gz', 'wt') as stream:
                    for r in items: stream.write(json.dumps(r)+'\n')
            write(rows)
            captured, native = S.evidence(root, report)
            self.assertEqual(captured[0, 2][1], dict(tick=0, planet=1, form='ship'))
            self.assertIsNone(native)
            for bad in [rows[1:], [rows[0], rows[0], rows[2]], rows[:2]]:
                write(bad)
                with self.assertRaises(ValueError): S.evidence(root, report)
            rows[1]['observation']['local']['combat']['recovery']['flight']['pilot']['owner'] = 'player_2'
            write(rows)
            with self.assertRaises(ValueError): S.evidence(root, report)

    def test_every_public_report_copy_is_audited(self):
        f, row, native = fixture()
        snapshot = dict(source_tick=10, candidates=[dict(destination=1, forecast=f)])
        source = dict(seat=0, source_tick=10, initial=copy.deepcopy(snapshot),
            published=copy.deepcopy(snapshot), last_snapshot=copy.deepcopy(snapshot))
        report = dict(transfer_comparison=dict(sources=[source]))
        with patch.object(S, 'evidence', return_value=({(0, 10):(row, None)}, native)):
            result = S.audit(None, report)
            self.assertEqual({r['view'] for r in result['records']}, {'initial', 'published', 'last_snapshot'})
            for view in ['initial', 'published', 'last_snapshot']:
                changed = copy.deepcopy(report)
                changed['transfer_comparison']['sources'][0][view]['candidates'][0]['forecast']['scan_clock']['opportunity_tick'] = 32
                with self.subTest(view=view), self.assertRaises(ValueError): S.audit(None, changed)

    def test_raw_probe_and_destination_cover_are_part_of_parity(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory)/'a', Path(directory)/'b'
            a.mkdir(); b.mkdir()
            for root in [a, b]:
                for name in ['transfer-comparison-work.jsonl', 'arrival-survey.jsonl', 'surveyed-arrival-comparison.jsonl']:
                    (root/name).write_text('')
            report = dict(transfer_comparison={})
            with patch.object(S.N.L.D, 'unchanged', return_value={}), patch.object(S.N.M.S, 'sensor_digest', return_value='same'):
                S.parity(a, b, report, report)
                for file in ['transfer-probe.jsonl', 'destination-cover.jsonl']:
                    (a/file).write_text('original')
                    (b/file).write_text('changed')
                    with self.subTest(file=file), self.assertRaises(AssertionError): S.parity(a, b, report, report)
                    (a/file).unlink(); (b/file).unlink()

    def test_later_choice_change_does_not_rewrite_an_earlier_scan(self):
        for changed_tick in [1, 3]:
            rows = []
            for tick in range(5):
                r = acquisition_row(tick)
                p, m = S.W.P.pilot(r), r['mission']
                p['ship_health'] = 100
                p['site_query'] = 'survey' if tick in [2, 4] else 'not_requested' if tick <= 1 else {'deferred': {'next_tick': 4}}
                p['planet']['claim'].update(planet=0, owner=None, flag=None,
                    claimant='player_1' if tick == changed_tick else None)
                r['observation']['planets'] = [copy.deepcopy(p['planet'])]
                site = dict(planet=0, bearing=12) if tick == 4 else None
                m.update(recovery=None, events=[dict(tick=1, planet=0, kind='arrived')])
                m['capture'].update(site=site, started_tick=4 if site else None,
                    failure=None, completed_tick=None, acquisition=dict(tick=tick, planet=0,
                        revision=0, selected_site=site, reason='selected_site' if site else 'candidates_rejected', checks={}))
                rows.append(r)
            report = dict(transfer_comparison=dict(sources=[dict(seat=0, source_tick=0, last_snapshot={})]),
                transfer_probe=dict(seat=0, destination=0, outcome=dict(reason='arrived', tick=1),
                    acquisition=dict(schema=1, started_tick=1, initial_replans=0, horizon_ticks=1800,
                        observed_rows=4, last_observed_tick=4, outcome=dict(tick=4, elapsed_ticks=3,
                            controller_observed=True, reason='site_selected', site=dict(planet=0, bearing=12)))))
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with gzip.open(root/'trace.jsonl.gz', 'wt') as stream:
                    for r in rows: stream.write(json.dumps(r)+'\n')
                _, native = S.evidence(root, report)
                self.assertEqual(native['first_scan'], 2)
                self.assertEqual(native['choice'], 4)
                self.assertEqual(native['source_continuity']['0:0'], changed_tick == 3)
                self.assertEqual(native['neutral_material'], changed_tick == 3)


if __name__ == '__main__': unittest.main()
