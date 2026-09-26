import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "capture_destinations", Path(__file__).resolve().parents[1] / "compare-capture-destinations.py")
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


def fixture():
    report = {field: [] for field in TOOL.PHYSICAL_FIELDS}
    report.update(
        physics_ok=True, elapsed_ticks=1000, initial_world={"seed": 42},
        round={"outcome": {"winner": "player_2"}, "owned_planets": [2, 1],
               "pilots": [{"health": 0, "death_tick": 1000}, {"health": 75, "death_tick": None}]},
        missions=[dict(completed_sorties=0, completed_recoveries=0) for _ in range(2)],
        metrics=[dict(visits=[], first_all_owned_tick=None, longest_phase_ticks={}) for _ in range(2)],
        final_pilots=[dict(recovery={"ships_lost": 1}, location="on_foot") for _ in range(2)],
        live_objective_planning={"destination_cover": {}}, mission_evaluation={"charged": 4},
        policy_configuration=[{"policy": "material_mission_v10"} for _ in range(2)],
        events=[], samples=[], pilot_damage_events=[],
    )
    return report


class FinishedMatchAccounting(unittest.TestCase):
    def test_result_follows_authoritative_winner_not_planet_count_or_alive_state(self):
        report = fixture()
        self.assertEqual(TOOL.outcome(report, 0), "loss")
        self.assertEqual(TOOL.outcome(report, 1), "win")
        report["round"]["outcome"] = "draw"
        self.assertEqual(TOOL.outcome(report, 0), "draw")
        report["round"]["outcome"] = None
        self.assertEqual(TOOL.outcome(report, 0), "unfinished")

    def test_each_players_survival_is_retained_and_repeated_switch_snapshots_are_deduplicated(self):
        report = fixture()
        first = {"tick": 20, "from": 0, "to": 1}
        last = {"tick": 50, "from": 1, "to": 0}
        def mission(count, switch):
            return {"destination_planning": {"switches": count, "last_switch": switch}}
        report["events"] = [{"seat": 1, "telemetry": mission(1, first)}] * 2
        report["missions"][1].update(mission(2, last))
        report["samples"] = [{"missions": [{}, mission(2, last)]}]
        p1, p2 = [TOOL.finished_player(report, seat) for seat in range(2)]
        self.assertEqual((p1["death_tick"], p1["owned_planets"], p1["outcome"]), (1000, 2, "loss"))
        self.assertEqual((p2["death_tick"], p2["pilot_health"], p2["outcome"]), (None, 75, "win"))
        self.assertEqual(p2["switches"], [first, last])
        report["events"] = []
        with self.assertRaises(AssertionError):
            TOOL.finished_player(report, 1)

    def test_counterfactual_checks_the_replaced_seat_and_rejects_unexplained_physical_changes(self):
        baseline = fixture()
        candidate = copy.deepcopy(baseline)
        candidate["policy_configuration"][1]["policy"] = "material_mission_v12"
        candidate["missions"][1]["destination_planning"] = {"switches": 0}
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            for name in ["baseline", "candidate"]:
                (out / name).mkdir()
            def compare():
                for name, data in [("baseline", baseline), ("candidate", candidate)]:
                    (out / name / "report.json").write_text(json.dumps(data))
                return TOOL.finished_comparison(out, "baseline", "candidate", 1)
            self.assertEqual(compare()["candidate_outcome"], "win")
            self.assertTrue(compare()["matching_recorded_physical_outcomes"])
            candidate["pilot_damage_events"] = [{"seat": 0, "tick": 40}]
            with self.assertRaises(AssertionError):
                compare()
            candidate["missions"][1]["destination_planning"]["switches"] = 1
            self.assertFalse(compare()["matching_recorded_physical_outcomes"])
            candidate["initial_world"]["seed"] = 43
            with self.assertRaises(AssertionError):
                compare()
            candidate["initial_world"]["seed"] = 42
            candidate["policy_configuration"].reverse()
            with self.assertRaises(AssertionError):
                compare()

    def test_damage_comparison_ignores_policy_labels_but_retains_physical_damage(self):
        baseline = fixture()
        baseline["pilot_damage_events"] = [{
            "tick": 50, "seat": 1, "vitals": {"health": 75, "death_tick": None},
            "mission": {"policy": "material_mission_v10"},
        }]
        candidate = copy.deepcopy(baseline)
        candidate["pilot_damage_events"][0]["mission"] = {
            "policy": "material_mission_v12", "destination_planning": {"switches": 0}}
        self.assertTrue(TOOL.same_physical_outcomes(baseline, candidate))
        candidate["pilot_damage_events"][0]["vitals"]["health"] = 74
        self.assertFalse(TOOL.same_physical_outcomes(baseline, candidate))

    def test_partial_comparisons_and_other_players_reports_are_not_counted_as_choices(self):
        partial = dict(actor="player_1", inactive_reason=None, preferred_by_time=None, current_target=0,
                       candidates=[{"total_seconds": 30}, {"total_seconds": 10},
                                   {"total_seconds": None, "unknown_reason": "unmeasured"}])
        complete = dict(partial, actor="player_2", preferred_by_time=1, candidates=partial["candidates"][:2])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evaluations.jsonl"
            path.write_text("\n".join(json.dumps(r) for r in [partial, complete]))
            p1, p2 = TOOL.evaluation_coverage(path)
        self.assertEqual(p1["counts"]["multiple_numeric_destinations"], 1)
        self.assertEqual(p1["counts"].get("complete_preferences", 0), 0)
        self.assertEqual(p1["unknown_candidates"], {"unmeasured": 1})
        self.assertEqual(p2["counts"]["alternative_preferences"], 1)


if __name__ == "__main__":
    unittest.main()
