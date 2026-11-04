"""Local synchronous protocol. Rust is the only physics/outcome implementation."""
import json
import subprocess
from pathlib import Path


class Bridge:
    def __init__(self, executable):
        self.executable = str(Path(executable).resolve())
        self.process = subprocess.Popen([self.executable], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)
        self.contract = self.call("schema")
        if self.contract["protocol"] != 2:
            raise ValueError("unsupported bridge protocol")

    def call(self, op, **values):
        self.process.stdin.write(json.dumps({"op": op, **values}, allow_nan=False, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f"flight bridge exited: {self.process.poll()}")
        response = json.loads(line)
        if not response["ok"]:
            raise RuntimeError(response["error"])
        return response["result"]

    def close(self):
        if self.process.poll() is None:
            try:
                self.call("close")
            finally:
                self.process.stdin.close()
                self.process.stdout.close()
                self.process.wait(timeout=10)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def task_pools(batch):
    if not batch["complete"]:
        raise RuntimeError("task generation quota incomplete")
    families = ["clear", "moving", "obstructed", "altered"]
    return [[r["candidate"] for r in batch["attempts"]
             if r["admitted"] and r["candidate"]["family"] == family] for family in families]
