#!/usr/bin/env python3
"""Three-minute recovery trials; retain reports and failures on desktop or Pi."""
import argparse
import itertools
import json
import pathlib
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--ssh")
    parser.add_argument("--ssh-option", action="append", default=[])
    parser.add_argument("--remote-out", default="/tmp/pod-impact-trials")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    ssh = ["ssh", "-F", "/dev/null", "-o", "BatchMode=yes"]
    for option in args.ssh_option:
        ssh += ["-o", option]
    if args.ssh:
        ssh.append(args.ssh)

    # Preserve the original full journey, then isolate repeated impacts early
    # enough for the entire two-minute recovery budget to fit in each run.
    cases = [(seed, seat, angled, "sortie", "none", "ejection")
             for seed, seat, angled in itertools.product([7, 42], range(2), [False, True])]
    cases += [(42, seat, angled, "airborne", "none", "ejection")
              for seat, angled in itertools.product(range(2), [False, True])]
    cases += [(42, seat, angled, "airborne", hazard, phase)
              for seat, angled, hazard, phase in itertools.product(
                  range(2), [False, True], ["light", "heavy", "missile"], ["ejection", "approach"])]
    results = []
    for seed, seat, angled, start, hazard, phase in cases:
        name = f"seed{seed}-seat{seat}-angle{int(angled)}-{start}-{hazard}-{phase}"
        out = args.out / name
        out.mkdir(exist_ok=True)
        run_out = f"{args.remote_out}/{name}" if args.ssh else str(out.resolve())
        command = [args.binary, "--seconds", "180", "--seed", str(seed), "--seat", str(seat),
                   "--oblique", str(angled).lower(), "--start", start, "--followup", hazard,
                   "--followup-phase", phase, "--out", run_out]
        if args.ssh:
            command = ssh + [shlex.join(command)]
        with (out / "run.log").open("w") as log:
            run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120)
        if args.ssh:
            copied = subprocess.run(ssh + [shlex.join(["cat", run_out + "/report.json"])],
                                    capture_output=True, timeout=15, check=True)
            (out / "report.json").write_bytes(copied.stdout)
        report = json.loads((out / "report.json").read_text())
        recovery = report["brain"]["recovery"] or {}
        stabilization = recovery.get("stabilization") or {}
        assert report["seconds"] == 180 and len(report["samples"]) == 180
        result = dict(name=name, exit_code=run.returncode, complete=report["complete"],
                      audit_passed=report["audit_passed"], reason=recovery.get("reason"),
                      recovery_status=recovery.get("status"),
                      departed_tick=report["brain"]["departed_tick"],
                      followup_tick=report["followup_tick"],
                      followup_contact_tick=report["followup_contact_tick"],
                      stabilization=stabilization,
                      max_speed=max(s["audit"]["max_speed"] for s in report["samples"]),
                      max_spin=max(s["audit"]["max_spin"] for s in report["samples"]),
                      step_p95_ms=report["step_p95_ms"], ai_p95_ms=report["ai_p95_ms"],
                      report=str(out / "report.json"))
        results.append(result)
        (args.out / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps(result), flush=True)
    raise SystemExit(int(any(r["exit_code"] != 0 for r in results)))


if __name__ == "__main__":
    main()
