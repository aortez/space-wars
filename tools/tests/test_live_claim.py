import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('claim', Path(__file__).resolve().parents[1] / 'diagnose-live-claim.py')
X = importlib.util.module_from_spec(spec); spec.loader.exec_module(X)
spec = importlib.util.spec_from_file_location('factor_fixtures', Path(__file__).with_name('test_integration_factors.py'))
F = importlib.util.module_from_spec(spec); spec.loader.exec_module(F)


def report(item):
    data = F.report(item)
    data.update(elapsed_ticks=1, missions=[{}, {}])
    if item['mask'] == 7:
        data['powered_capture']['live_claim_stopping_enabled_seats'] = [
            s == item['seat'] and item['live_claim_stopping'] for s in (0, 1)]
    if not item['live_claim_stopping']:
        data['missions'][item['seat']]['live_claim_stopping_disabled'] = True
    return data


def rows(item):
    result = []
    for seat in (0, 1):
        disabled = not item['live_claim_stopping'] and seat == item['seat']
        result.append(dict(seat=seat, mission=dict(live_claim_stopping_disabled=disabled,
            capture=dict(ground=dict(live_claim_stopping_disabled=disabled,
                continuous_walk=item['mask'] == 7 and seat == item['seat']))),
            observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=dict(
                balanced=True, supported_planet=1, owner='player_1', planet=dict(index=1,
                claim=dict(owner=None, claimant='player_1', phase='raising', status='raising',
                           flag=dict(player='player_1')))))))))))
    return result


class LiveClaimTests(unittest.TestCase):
    def test_three_arms_keep_all_selected_worlds_seats_and_complete_match_options(self):
        cases = X.cases(F.source())
        self.assertEqual(len(cases), 12)
        self.assertEqual(len({c['name'] for c in cases}), 12)
        for group in X.Q.GROUPS:
            selected = [c for c in cases if c['group'] == group]
            self.assertEqual([c['configuration'] for c in selected], list(X.CONFIGS))
            self.assertEqual(len({c['seed'] for c in selected}), 1)
            original = F.source()['runs'][group + '-control']['item']
            for item in selected:
                for key in ('seed', 'seat', 'opponent', 'interval', 'world', 'seconds'):
                    self.assertEqual(item[key], original[key])
                options = X.flags(item)
                self.assertEqual((options['--seconds'], options['--require-finish']), ('600', 'true'))

    def test_ablation_changes_exactly_one_option_and_preserves_both_endpoints(self):
        for group in X.Q.GROUPS:
            items = {c['configuration']: c for c in X.cases(F.source()) if c['group'] == group}
            on, off = [X.flags(items[c]) for c in ('integrated', 'no-stop')]
            self.assertEqual({k for k in on if on[k] != off[k]}, {'--live-claim-stopping'})
            for config, original in (('ordinary', 'control'), ('integrated', 'candidate')):
                opts = X.flags(items[config]); self.assertEqual(opts.pop('--live-claim-stopping'), 'true')
                old = F.source()['runs'][group + '-' + original]['command']
                self.assertEqual([v for pair in opts.items() for v in pair], old[1:-2])

    def test_report_rejects_wrong_switch_seat_and_unintended_powered_disable(self):
        for item in X.cases(F.source()):
            data = report(item); X.check_configuration(data, item)
            if item['mask'] == 7:
                changed = copy.deepcopy(data)
                enabled = changed['powered_capture']['live_claim_stopping_enabled_seats']
                enabled[item['seat']] = not enabled[item['seat']]
                with self.assertRaises(AssertionError): X.check_configuration(changed, item)
                changed = copy.deepcopy(data); changed['active_flight_checks']['enabled_seats'] = [False, False]
                with self.assertRaises(AssertionError): X.check_configuration(changed, item)

    def test_dense_audit_detects_lost_reset_propagation_and_walking_mode(self):
        for item in X.cases(F.source()):
            data, trace = report(item), rows(item)
            X.audit_switch(iter(trace), data, item)
            for side in (0, 1):
                changed = copy.deepcopy(trace)
                ground = changed[side]['mission']['capture']['ground']
                ground['live_claim_stopping_disabled'] = not ground['live_claim_stopping_disabled']
                with self.assertRaises(AssertionError): X.audit_switch(iter(changed), data, item)
            if item['mask'] == 7:
                changed = copy.deepcopy(trace)
                changed[item['seat']]['mission']['capture']['ground']['continuous_walk'] = False
                with self.assertRaises(AssertionError): X.audit_switch(iter(changed), data, item)
            with self.assertRaises(AssertionError): X.audit_switch(iter(trace[:1]), data, item)

    def test_native_claim_classification_requires_support_and_correct_owner(self):
        item = X.cases(F.source())[2]; row = rows(item)[0]
        self.assertTrue(X.live_claim(row))
        for mutate in (lambda p: p.update(balanced=False), lambda p: p.update(supported_planet=None),
                       lambda p: p['planet']['claim'].update(owner='player_1'),
                       lambda p: p['planet']['claim'].update(status='contested'),
                       lambda p: p['planet']['claim']['flag'].update(player='player_2')):
            changed = copy.deepcopy(row); mutate(X.R.pilot(changed))
            self.assertFalse(X.live_claim(changed))

    def test_frozen_plan_rejects_changed_cases_binary_source_runtime_and_plan(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory); binary = out / 'binary'; binary.write_bytes(b'new main runtime')
            prior = F.source(); path = out / 'source.json'; path.write_text(json.dumps(prior))
            frozen = {'runtime.rs': 'runtime', X.OWN[0]: 'runner', X.OWN[2]: 'plan'}
            plan = dict(profile=X.PROFILE, base_commit=X.BASE, inputs=frozen,
                source=dict(path=str(path), sha256=X.P.digest(path)),
                jobs=X.jobs(prior, binary, out), binary=dict(path=str(binary), sha256=X.P.digest(binary)))
            with patch.object(X, 'inputs', return_value=frozen):
                X.verify_plan(plan, out)
                for mutate in (lambda p: p['jobs'].pop(), lambda p: p['jobs'][2]['item'].update(live_claim_stopping=True),
                               lambda p: p.update(base_commit='other'), lambda p: p['binary'].update(sha256='wrong')):
                    changed = copy.deepcopy(plan); mutate(changed)
                    with self.assertRaises(AssertionError): X.verify_plan(changed, out, True)
            with patch.object(X, 'inputs', return_value={**frozen, X.OWN[0]: 'new auditor'}):
                with self.assertRaises(AssertionError): X.verify_plan(plan, out)
                X.verify_plan(plan, out, True)
            for key in ('runtime.rs', X.OWN[2]):
                with patch.object(X, 'inputs', return_value={**frozen, key: 'changed'}):
                    with self.assertRaises(AssertionError): X.verify_plan(plan, out, True)

    def test_decision_requires_all_contrasts_and_cannot_promote(self):
        prior = F.source(); cases = X.cases(prior); runs = {c['name']: dict(item=c, audited=True) for c in cases}
        pairs = [dict(group=g, before_configuration=a, after_configuration=b,
                      factor='live_claim_stopping' if a == 'integrated' else 'integration',
                      first_control_difference=None, outcome_transition='win->win')
                 for g in X.Q.GROUPS for a, b in X.CONTRASTS]
        result = X.diagnosis(runs, pairs, prior)
        self.assertFalse(result['default_promotion'])
        self.assertEqual(result['fresh_games'], 0)
        with self.assertRaises(AssertionError): X.diagnosis(runs, pairs[:-1], prior)
        changed = copy.deepcopy(pairs); changed[-1] = changed[0]
        with self.assertRaises(AssertionError): X.diagnosis(runs, changed, prior)


if __name__ == '__main__': unittest.main()
