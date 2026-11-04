"""Correctness preflight, not a flight capability evaluation."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import torch
from torch.distributions import Normal, TransformedDistribution, TanhTransform, SigmoidTransform
from .bridge import Bridge, task_pools
from .checkpoint import identity, load
from .model import FlightModel, flatten, tensor, actions
from .ppo import gae
from .reward import potential, reward

BRIDGE = os.environ.get("FLIGHT_PPO_BRIDGE", "target/ci/examples/flight_ppo_bridge")


def equal_tree(a, b):
    if isinstance(a, torch.Tensor):
        return isinstance(b, torch.Tensor) and torch.equal(a, b)
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(equal_tree(a[k], b[k]) for k in a)
    if isinstance(a, (tuple, list)):
        return len(a) == len(b) and all(equal_tree(x, y) for x, y in zip(a, b))
    return a == b


class PolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        torch.manual_seed(42)
        cls.bridge = Bridge(BRIDGE)
        cls.schema = cls.bridge.contract["schema"]
        cls.pools = task_pools(cls.bridge.call("generate", seed=3110001, per_family=1, max_rounds=128))
        cls.items = cls.bridge.call("reset", tasks=[p[0]["task"] for p in cls.pools])["items"]

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def test_masked_set_permutation_payload_and_empty_set(self):
        model = FlightModel(self.schema)
        x = tensor(self.items)
        reference = model(x)
        changed = x.clone()
        objects = changed[:, 3497:].reshape(-1, 32, 16)
        objects[:] = objects[:, torch.randperm(32)].clone()
        absent = objects[:, :, 0] == 0
        objects[:, :, 1:][absent] = 1e5
        actual = model(changed)
        for a, b in zip(actual, reference):
            torch.testing.assert_close(a, b, atol=2e-6, rtol=2e-6)
        changed[:, 3497:] = 0
        self.assertTrue(all(torch.isfinite(v).all() for v in model(changed)))

    def test_clear_control_recipe_bounds_admission_and_partitions(self):
        from .curriculum import candidate, generate, STRATA
        import math
        for index in range(128):
            a = candidate(4110001, index)
            self.assertEqual(a, candidate(4110001, index))
            self.assertNotEqual(a["task"], candidate(4110002, index)["task"])
            task = a["task"]
            low, high, speed, spin = STRATA[index % 4]
            distance = math.hypot(*task["goal_offset"].values())
            self.assertGreaterEqual(distance, low)
            self.assertLessEqual(distance, high)
            self.assertLessEqual(math.hypot(*task["start"]["relative_velocity"].values()), speed)
            self.assertLessEqual(abs(task["start"]["spin"]), spin)
            self.assertFalse(task["start"]["gravity"])
            self.assertEqual(task["arrival"], {"position": 3., "velocity": 1., "spin": .2, "dwell_ticks": 30})
        before = self.bridge.call("state")
        batch, pools = generate(self.bridge, 4110001, 2)
        self.assertTrue(batch["complete"])
        self.assertEqual([len(p) for p in pools], [2]*4)
        for report in batch["attempts"]:
            if report["admitted"]:
                self.assertTrue(report["evidence"]["direct_clear"])
                self.assertTrue(report["evidence"]["goal_trajectory_clear"])
        self.assertEqual(before, self.bridge.call("state"))
        limited, _ = generate(self.bridge, 4110001, 2, max_rounds=1)
        self.assertFalse(limited["complete"])
        self.assertEqual(len(limited["attempts"]), 4)

    def test_clear_control_gates_require_coverage_and_multiple_seeds(self):
        from .learning_metrics import seed_gates, aggregate
        baseline = [{"recipe": f"clear-control-v1/stratum-{i}", "outcome": "deadline"} for i in range(4) for _ in range(4)]
        final = copy.deepcopy(baseline)
        for i, row in enumerate(final):
            row["outcome"] = "success" if i % 4 < 3 else "deadline"
        passed = seed_gates(baseline, final)
        self.assertTrue(passed["basic_flight_gate"])
        failed = seed_gates(baseline, baseline)
        self.assertFalse(aggregate([passed, failed, failed])["learning_evidence"])
        self.assertTrue(aggregate([passed, passed, failed])["basic_flight_gate"])
        for i in range(4):
            final[i]["outcome"] = "deadline"
        self.assertFalse(seed_gates(baseline, final)["basic_flight_gate"])
        with self.assertRaises(ValueError):
            seed_gates(baseline, final[:-1])

    def test_circular_convolution_wraps_angular_seam(self):
        model = FlightModel(self.schema)
        x = torch.randn(2, 4, 23, 64)
        a = model.polar_conv(model.conv1, x.roll(2, dims=-1))
        b = model.polar_conv(model.conv1, x).roll(1, dims=-1)
        torch.testing.assert_close(a, b, atol=1e-6, rtol=1e-6)

    def test_joint_log_probability_matches_transformed_distribution(self):
        model = FlightModel(self.schema)
        actor = torch.tensor([[0.2, 0.8, -0.7], [-0.4, -0.1, 0.2]])
        u = torch.tensor([0.7, -1.3])
        actuators = torch.tensor([[1., -0.3], [-0.7, 0.2]])
        dist = TransformedDistribution(Normal(actor[:, 0], model.log_std.exp()[0]), [TanhTransform(cache_size=1)])
        actuator_dist = TransformedDistribution(Normal(actor[:, 1:], model.log_std.exp()[1:]), [SigmoidTransform()])
        expected = dist.log_prob(u.tanh()) + actuator_dist.log_prob(actuators.sigmoid()).sum(-1)
        torch.testing.assert_close(model.log_prob(actor, u, actuators), expected, atol=1e-6, rtol=1e-6)
        self.assertTrue(torch.isfinite(model.log_prob(actor, torch.tensor([-40., 40.]), torch.tensor([[-80.,80.],[80.,-80.]]))).all())

    def test_continuous_actions_have_no_button_threshold_and_old_exports_are_rejected(self):
        commands = actions(torch.tensor([-.5,0.,.5]), torch.tensor([[-2.,2.],[0.,0.],[2.,-2.]]))
        self.assertLess(commands[0]['thrust'], commands[1]['thrust'])
        self.assertLess(commands[1]['thrust'], commands[2]['thrust'])
        self.assertEqual(commands[1], {'steering':0.,'thrust':.5,'brake':.5})
        self.assertTrue(all(0 < c['thrust'] < 1 and 0 < c['brake'] < 1 for c in commands))
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'old.json'
            FlightModel(self.schema).export(path)
            record = json.loads(path.read_text());record['format_version'] = 1
            path.write_text(json.dumps(record))
            with self.assertRaisesRegex(RuntimeError, 'incompatible flight policy'):
                self.bridge.call('load_policy', path=str(path))
        with self.assertRaises(ValueError):FlightModel(dict(self.schema,version=1))

    def test_gae_stops_on_terminal_but_bootstraps_rollout_boundary(self):
        rewards = torch.tensor([[1.], [2.], [3.]])
        dones = torch.tensor([[0.], [1.], [0.]])
        values = torch.tensor([[10.], [20.], [30.]])
        adv, returns = gae(rewards, dones, values, torch.tensor([40.]), 0.9, 1.0)
        torch.testing.assert_close(returns, torch.tensor([[2.8], [2.], [39.]]))
        torch.testing.assert_close(adv, returns-values)

    def test_potential_telescopes_through_detours_and_all_terminals(self):
        frames = [{"errors": {"position": d, "velocity": 10., "spin": 1.}} for d in [80., 120., 80., 2.]]
        gamma = 0.9995
        for outcome in ["success", "collision", "deadline"]:
            total = 0
            for i in range(3):
                terms = reward({"frame": frames[i]}, {"frame": frames[i+1], "progress": {"outcome": outcome if i == 2 else None}}, gamma)
                total += gamma**i * terms["potential"]
            self.assertAlmostEqual(total, -potential(frames[0]), places=6)
        # A full deadline is preferable to deliberate early collision under the
        # declared base reward; a last-tick success beats a full timeout.
        time_cost = -0.001 * sum(gamma**i for i in range(1800))
        timeout = time_cost - 2 * gamma**1799
        self.assertGreater(timeout, -10.001)
        self.assertGreater(time_cost + 10 * gamma**1799, timeout)
        with self.assertRaises(RuntimeError):
            reward({"frame": frames[0]}, {"frame": frames[0], "progress": {"outcome": "environment_error"}}, gamma)

    def test_rust_export_parity_and_malformed_policy_rejection(self):
        model = FlightModel(self.schema)
        fixtures = [item["inputs"] for item in self.items]
        for _ in range(12):
            item = copy.deepcopy(fixtures[0])
            item["self_goal"] = torch.randn(35).tolist()
            item["cells"] = torch.rand(1472, 2).tolist()
            item["rays"] = torch.randn(128, 4).tolist()
            item["nearest"] = torch.randn(6).tolist()
            item["objects"] = torch.randn(32, 16).tolist()
            for i, row in enumerate(item["objects"]):
                row[0] = float(i < 12)
            fixtures.append(item)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "model.json"
            model.export(path)
            self.bridge.call("load_policy", path=str(path))
            native = self.bridge.call("infer", inputs=fixtures)["outputs"]
            with torch.no_grad():
                actor, value = model(torch.tensor([flatten(x) for x in fixtures]))
            expected = torch.cat((actor, value[:, None]), dim=1)
            actual = torch.tensor([[r["steering_latent"], r["thrust_logit"], r["brake_logit"], r["value"]] for r in native])
            torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
            for output, row in zip(native, actor):
                self.assertAlmostEqual(output["action"]["steering"], float(row[0].tanh()), places=5)
                self.assertAlmostEqual(output["action"]["thrust"], float(row[1].sigmoid()), places=5)
                self.assertAlmostEqual(output["action"]["brake"], float(row[2].sigmoid()), places=5)
            malformed = json.loads(path.read_text())
            malformed["weights"]["actor.weight"]["values"].pop()
            path.write_text(json.dumps(malformed))
            with self.assertRaises(RuntimeError):
                self.bridge.call("load_policy", path=str(path))
            model.export(path)
            incompatible = json.loads(path.read_text())
            incompatible["observation_schema"]["radial_edges"][1] += 0.001
            path.write_text(json.dumps(incompatible))
            with self.assertRaises(RuntimeError):
                self.bridge.call("load_policy", path=str(path))

    def test_bridge_replay_and_batch_validation_are_transactional(self):
        self.bridge.call("reset", tasks=[p[0]["task"] for p in self.pools])
        for _ in range(3):
            self.bridge.call("step", actions=[{"steering": .3, "thrust": .3, "brake": .6}] * 4)
        state = self.bridge.call("state")
        expected = self.bridge.call("restore", **state)
        self.assertEqual(expected, self.bridge.call("restore", **json.loads(json.dumps(state))))
        with self.assertRaises(RuntimeError):
            self.bridge.call("step", actions=[])
        self.assertEqual(state, self.bridge.call("state"))
        corrupt = copy.deepcopy(state)
        corrupt["traces"][0]["initial"]["spin"] += 1
        with self.assertRaises(RuntimeError):
            self.bridge.call("restore", **corrupt)
        self.assertEqual(state, self.bridge.call("state"))

    def test_fresh_process_checkpoint_continuation_is_bitwise_equal(self):
        config = json.loads((Path(__file__).parent / "smoke-config.json").read_text())
        config.update(updates=2, rollout_ticks=8, epochs=2, minibatch=16, train_tasks_per_family=1)
        self.check_continuation(config)

    def test_clear_control_checkpoint_continuation_is_bitwise_equal(self):
        config = json.loads((Path(__file__).parent / "clear-control-config.json").read_text())
        config.update(updates=2, rollout_ticks=8, epochs=2, minibatch=16, train_tasks_per_stratum=1)
        self.check_continuation(config)

    def check_continuation(self, config):
        from contextlib import nullcontext
        retained = os.environ.get("FLIGHT_CONTINUOUS_TEST_ARTIFACTS")
        context = nullcontext(tempfile.mkdtemp(prefix="resume-", dir=retained)) if retained else tempfile.TemporaryDirectory()
        with context as tmp:
            root = Path(tmp)
            config_path = root / "config.json"
            config_path.write_text(json.dumps(config))
            command = [sys.executable, "-m", "tools.flight_ppo.run", "--bridge", BRIDGE, "--config", str(config_path), "--skip-evaluation"]
            subprocess.run(command + ["--out", str(root / "continuous")], check=True, capture_output=True, text=True)
            checkpoint = root / "continuous/checkpoint-0001.pt"
            subprocess.run(command + ["--out", str(root / "resumed"), "--resume", str(checkpoint)], check=True, capture_output=True, text=True)
            a = torch.load(root / "continuous/checkpoint-0002.pt", weights_only=True)
            b = torch.load(root / "resumed/checkpoint-0002.pt", weights_only=True)
            self.assertTrue(equal_tree(a, b), "weights/optimizer/RNG/collector/update history differ")
            initial = torch.load(root / "continuous/checkpoint-0000.pt", weights_only=True)
            for prefix in ["conv1", "conv2", "object_fc1", "actor", "critic"]:
                self.assertFalse(torch.equal(initial["model"][prefix+".weight"], a["model"][prefix+".weight"]), prefix)
            altered = dict(config, gamma=0.5)
            with self.assertRaises(ValueError):
                load(checkpoint, identity(altered, self.bridge), self.bridge)


if __name__ == "__main__":
    unittest.main(verbosity=2)
