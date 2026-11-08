"""Continuation gates must keep the original and boundary baselines distinct."""
import copy
from pathlib import Path
import tempfile
import unittest
from tools.flight_audit.rate_continuation import extension_gate,block_outcomes,materialize_history
from tools.flight_audit.short_recovery import progression
from tools.flight_audit.short_recovery_tests import records


def fixtures():
    rows=records()
    for r in rows:r['boundary_primary']={'successes':8}
    return rows


class ContinuationTests(unittest.TestCase):
    def test_gain_and_retention_must_coincide_in_two_seeds(self):
        rows=fixtures();self.assertTrue(extension_gate(rows)['passed'])
        rows[0]['legacy_hold']['final']['immediate_holds']=11
        rows[1]['boundary_primary']['successes']=12
        self.assertFalse(extension_gate(rows)['passed'])

    def test_no_gain_or_unsafe_recovery_cannot_pass(self):
        rows=fixtures()
        for r in rows:r['primary']['final']['successes']=8
        self.assertFalse(extension_gate(rows)['passed'])
        rows=fixtures()
        for r in rows[:2]:r['primary']['final']['physical_failures']=1
        self.assertFalse(extension_gate(rows)['passed'])

    def test_extension_pass_is_not_original_baseline_progression(self):
        rows=fixtures()
        for r in rows:r['primary']['initial']['successes']=13
        gate=extension_gate(rows);self.assertTrue(gate['passed']);self.assertEqual(gate['per_seed'][0]['gain_from_0'],-1)
        self.assertFalse(progression(rows)['passed'])

    def test_new_outcomes_exclude_completed_prefix_and_reject_rewrites(self):
        before={'update':16,'history':list(range(16)),'collector':{'steps':16384,'completed':[{'id':0,'outcome':'success'}]}}
        after={'update':32,'history':list(range(32)),'collector':{'steps':32768,'completed':[{'id':0,'outcome':'success'},{'id':1,'outcome':'deadline'},{'id':2,'outcome':'success'}]}}
        self.assertEqual(block_outcomes(before,after),{'deadline':1,'success':1})
        changed=copy.deepcopy(after);changed['collector']['completed'][0]['outcome']='deadline'
        with self.assertRaises(AssertionError):block_outcomes(before,changed)
        changed=copy.deepcopy(after);changed['history'][4]=100
        with self.assertRaises(AssertionError):block_outcomes(before,changed)

    def test_historical_trace_copy_never_overwrites_new_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);src=root/'old';dst=root/'new';src.mkdir();dst.mkdir()
            (src/'episode-000001.json').write_text('{"old":true}')
            self.assertEqual(materialize_history(src,dst),['episode-000001.json'])
            self.assertEqual((src/'episode-000001.json').read_bytes(),(dst/'episode-000001.json').read_bytes())
            (dst/'episode-000001.json').write_text('{"new":true}')
            with self.assertRaises(FileExistsError):materialize_history(src,dst)
            self.assertEqual((dst/'episode-000001.json').read_text(),'{"new":true}')


if __name__=='__main__':unittest.main()
