#!/usr/bin/env python3
"""Opt-in isolated serving autotuning, using the existing streaming evidence runner.

The BFF owns configuration writes and crash recovery. This process owns load
generation only; losing its lease stops decisions and eventually restores the baseline.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import time
import uuid
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, Request, build_opener

import run_benchmark as benchmark

TERMINAL = {"completed", "rolled_back", "conflict"}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


class ApiError(RuntimeError):
    pass


class WarmupError(ApiError):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, new_url):
        return None


class Api:
    def __init__(self, base_url, token):
        benchmark.validate_url(base_url)
        self.base_url = base_url.rstrip("/")
        self.token = token

    def request(self, path, body=None):
        headers = {"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"}
        data = None if body is None else benchmark.encoded_json(body)
        try:
            with build_opener(NoRedirect()).open(
                Request(self.base_url + path, data=data, headers=headers), timeout=15,
            ) as response:
                raw = response.read(4 * 1024 * 1024 + 1)
                if len(raw) > 4 * 1024 * 1024:
                    raise ApiError("control response exceeded its size limit")
                return json.loads(raw)
        except HTTPError as error:
            # Never print response text, request headers, tokens, or supplied prompts.
            raise ApiError(f"control request failed with HTTP {error.code}") from None


class Controller:
    def __init__(self, api, job, options):
        self.api = api
        self.job = job
        self.options = options
        self.closed = threading.Event()
        self.lease_lost = threading.Event()
        self.heartbeat = None

    @property
    def path(self):
        return f"/api/v2/autotune/{self.job['id']}"

    def command(self, action, **fields):
        if self.lease_lost.is_set():
            raise ApiError("controller lease could not be renewed; decisions stopped")
        return self.api.request(self.path, {
            "owner": self.job["owner"], "epoch": self.job["epoch"],
            "operation": {"action": action, **fields},
        })

    def start_heartbeat(self):
        def renew():
            while not self.closed.wait(20):
                try:
                    self.command("heartbeat")
                except Exception:
                    try:
                        if self.api.request(self.path)["phase"] in TERMINAL:
                            return
                    except Exception:
                        pass
                    self.lease_lost.set()
                    return
        self.heartbeat = threading.Thread(target=renew, name="autotune-lease", daemon=True)
        self.heartbeat.start()

    def wait_for_phase(self, phases, *, terminal_ok=False):
        deadline = time.monotonic() + self.options.startup_timeout
        while time.monotonic() < deadline:
            if self.lease_lost.is_set():
                raise ApiError("controller lease lost")
            self.job = self.api.request(self.path)
            phase = self.job["phase"]
            if phase in phases:
                return self.job
            if phase in TERMINAL:
                if terminal_ok:
                    return self.job
                raise ApiError(f"experiment stopped in phase {phase}")
            self.closed.wait(1)
        raise ApiError("timed out waiting for configuration reconciliation")

    def wait_for_endpoints(self, production=False):
        uid = self.job["model_uid"] if production else self.job["experiment_uid"]
        deadline = time.monotonic() + self.options.startup_timeout
        while time.monotonic() < deadline:
            if self.lease_lost.is_set():
                raise ApiError("controller lease lost")
            detail = self.api.request(f"/api/v2/models/{quote(uid, safe='')}")
            dep = detail.get("deployment") or {}
            plan = detail.get("placement") or {}
            assignments = plan.get("assignments", [])
            endpoints = detail.get("endpoints", [])
            expected = self.job["trial"]
            if production:
                expected = dict(self.job["baseline"])
                expected["config_overrides"] = {
                    **(expected.get("config_overrides") or {}),
                    "max_num_seqs": self.job["parameters"]["max_num_seqs"],
                    "max_num_batched_tokens": self.job["parameters"]["max_num_batched_tokens"],
                }
            # Rust retains legacy null fields when serializing ModelConfig.
            config = {key: value for key, value in (dep.get("config_overrides") or {}).items()
                      if value is not None}
            expected_config = {key: value for key, value in (expected.get("config_overrides") or {}).items()
                               if value is not None}
            current = (
                len(assignments) == self.job["baseline"]["replicas"]
                and config == expected_config
                and dep.get("version") == (
                    self.job["baseline"]["version"] + 1 if production else self.job["trial"]["version"]
                )
                and all(any(
                    ep["replica_id"] == assignment["replica_id"]
                    and ep["status"] == "ready"
                    and ep["plan_version"] == plan["version"]
                    for ep in endpoints
                ) for assignment in assignments)
            )
            # The server re-checks the actual CLI projection and fresh endpoints
            # when admitting each measurement; this is only an early wait.
            if current:
                return
            self.closed.wait(2)
        raise ApiError("trial engine did not become ready before its startup budget")

    def configure(self, candidate):
        self.job = self.command("configure", candidate=candidate)
        self.wait_for_phase({"ready"})
        self.wait_for_endpoints()

    def measure(self, candidate, position, *, production=False):
        uid = self.job["model_uid"] if production else self.job["experiment_uid"]
        objective = self.job["request"]["objective"]
        version = self.job["baseline"]["version"] + 1 if production else self.job["trial"]["version"]
        note = f"autotune:{self.job['id']}:{version}:{canonical(self.job['parameters'])}"
        gpu_count = sum(len(r["gpu_indices"]) for r in self.job["trial"]["replica_specs"])
        argv = [
            "--workload-file", self.options.workload_file, "--model", uid,
            "--engine", "vllm", "--engine-version", self.job["request"]["engine_version"],
            "--platform", self.job["platform"], "--gpu-name", self.job["gpu_name"],
            "--gpu-count", str(gpu_count), "--base-url", self.options.gateway_url,
            "--token-env", self.options.gateway_token_env, "--image-id", self.job["trial"]["image_id"],
            "--concurrency", str(self.options.concurrency), "--requests", str(self.options.requests),
            "--timeout", str(self.options.request_timeout), "--seed", str(self.options.seed),
            "--cache-state", "warm", "--comparison-group", self.job["id"],
            "--round", str((candidate or 0) + 1),
            "--notes", note, "--param-fingerprint", hashlib.sha256(note.encode()).hexdigest(),
            "--slo-ttft-ms", str(objective["ttft_p95_ms_max"]),
            "--slo-tpot-ms", str(objective["tpot_p95_ms_max"]),
            "--slo-e2e-ms", str(objective["e2e_p95_ms_max"]),
        ]
        if position is not None:
            argv += ["--abba-position", str(position)]
        args, workload = benchmark.parse_args(argv)
        plan = benchmark.request_plan(args, workload)
        workload["request_count"] = len(plan)
        # Replay the full workload once, never merely declare an unknown cache warm.
        warmup = benchmark.run_requests(args, plan)
        if not all(row["ok"] for row in warmup["samples"]):
            artifacts = Path(self.options.artifacts_dir) / self.job["id"] / args.run_id
            artifacts.mkdir(parents=True, exist_ok=False)
            (artifacts / "failed_warmup.json").write_text(canonical(warmup) + "\n")
            raise WarmupError("full-plan warmup failed; configuration cannot be compared")
        if self.lease_lost.is_set():
            raise ApiError("controller lease lost during warmup")
        metrics = benchmark.run_requests(args, plan)
        raw = b"".join(benchmark.encoded_json(row) + b"\n" for row in metrics["samples"])
        run = benchmark.build_run(
            args, workload, metrics, benchmark.hash_plan(plan), hashlib.sha256(raw).hexdigest(),
        )
        run["evidence"]["cache_state_source"] = "full_plan_warmup"
        artifacts = Path(self.options.artifacts_dir) / self.job["id"] / args.run_id
        artifacts.mkdir(parents=True, exist_ok=False)
        with (artifacts / "samples.jsonl").open("xb") as file:
            file.write(raw)
        with (artifacts / "manifest.json").open("x") as file:
            json.dump({"run": run, "warmup_requests": len(warmup["samples"])}, file, allow_nan=False)
            file.write("\n")
        self.api.request("/api/v2/benchmarks/runs", run)
        self.job = self.command("record", run_id=run["run_id"])
        return run

    def run(self):
        self.start_heartbeat()
        try:
            self.wait_for_phase({"ready"})
            self.wait_for_endpoints()
            for candidate in range(len(self.job["request"]["candidates"])):
                # A B B A: both baseline observations bracket the candidate,
                # using identical workload, generator concurrency, seed and SLO.
                for position, variant in enumerate([None, candidate, candidate, None]):
                    self.configure(variant)
                    try:
                        self.measure(candidate, position)
                    except WarmupError:
                        if variant is None:
                            raise
                        print(canonical({
                            "experiment_id": self.job["id"], "rejected_candidate": candidate,
                            "reason": "warmup_failed",
                        }), file=sys.stderr)
                        break
            winners = eligible_candidates(self.job)
            if not winners:
                self.job = self.command("finish")
                return self.wait_for_phase(TERMINAL, terminal_ok=True)
            winner = winners[0]
            self.job = self.command("promote", candidate=winner)
            self.wait_for_phase({"observing"})
            self.wait_for_endpoints(production=True)
            started = time.monotonic()
            observations = 0
            duration = self.job["request"]["observation_seconds"]
            while observations < 2 or time.monotonic() - started < duration:
                self.measure(winner, None, production=True)
                observations += 1
                if self.job["phase"] != "observing":
                    return self.wait_for_phase(TERMINAL, terminal_ok=True)
                self.closed.wait(max(5, duration / 24))
            self.job = self.command("finish")
            return self.wait_for_phase(TERMINAL, terminal_ok=True)
        except BaseException:
            try:
                if not self.lease_lost.is_set():
                    self.command("rollback")
            except Exception:
                # A pending intent or failed connection is recovered after lease
                # expiry by the BFF, including when this process is killed.
                pass
            raise
        finally:
            self.closed.set()
            if self.heartbeat is not None:
                self.heartbeat.join(timeout=16)


def eligible_candidates(job):
    objective = job["request"]["objective"]

    def score(run):
        evidence = run.get("evidence") or {}
        values = [
            evidence.get("goodput_rps"), run.get("ttft_p95_ms"),
            run.get("tpot_p95_ms"), evidence.get("e2e_p95_ms"),
        ]
        if (
            run.get("status") != "succeeded" or run.get("error_rate") != 0
            or evidence.get("synthetic") is not False
            or evidence.get("request_count", 0) < objective["min_requests"]
            or evidence.get("successful_requests") != evidence.get("request_count")
            or not all(benchmark.finite_number(x, positive=True) for x in values)
            or values[1] > objective["ttft_p95_ms_max"]
            or values[2] > objective["tpot_p95_ms_max"]
            or values[3] > objective["e2e_p95_ms_max"]
        ):
            return None
        return values[0]

    eligible = []
    for index in range(len(job["request"]["candidates"])):
        group = [m for m in job["measurements"] if not m["production"]
                 and m["run"]["evidence"].get("round") == index + 1]
        baseline = [score(m["run"]) for m in group if m["candidate"] is None]
        candidate = [score(m["run"]) for m in group if m["candidate"] == index]
        if len(baseline) == len(candidate) == 2 and None not in baseline + candidate:
            if min(candidate) > max(baseline) * (1 + objective["min_improvement"]):
                eligible.append((min(candidate), index))
    return [index for _, index in sorted(eligible, reverse=True)]


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, help="Production model UID, not an alias")
    parser.add_argument("--config", required=True, help="Authorized candidates, resources and objective JSON")
    parser.add_argument("--workload-file", required=True, help="Explicit, non-sensitive representative workload")
    parser.add_argument("--bff-url", required=True)
    parser.add_argument("--gateway-url", required=True, help="Gateway OpenAI base URL, including /v1")
    parser.add_argument("--bff-token-env", default="NEBULA_BFF_TOKEN")
    parser.add_argument("--gateway-token-env", default="NEBULA_GATEWAY_TOKEN")
    parser.add_argument("--artifacts-dir", required=True)
    parser.add_argument("--concurrency", type=int, required=True)
    parser.add_argument("--requests", type=int, default=32)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--request-timeout", type=float, default=60)
    parser.add_argument("--startup-timeout", type=float, default=900)
    parser.add_argument("--cycles", type=int, default=1, help="0 repeats until interrupted")
    parser.add_argument("--interval", type=float, default=600, help="Cooldown between optimization cycles")
    args = parser.parse_args(argv)
    for url in [args.bff_url, args.gateway_url]:
        benchmark.validate_url(url)
    if not 1 <= args.concurrency <= 4096 or not 16 <= args.requests <= 100_000:
        parser.error("invalid concurrency or request budget")
    if not all(benchmark.finite_number(x, positive=True) for x in [
        args.request_timeout, args.startup_timeout, args.interval,
    ]) or args.interval < 300 or args.cycles < 0:
        parser.error("timeouts must be positive; cooldown >= 300s; cycles >= 0")
    for path in [args.config, args.workload_file]:
        if Path(path).stat().st_size > 1024 * 1024:
            parser.error("config and workload files must not exceed 1 MiB")
    return args


def main(argv=None):
    options = parse_args(argv)
    try:
        token = os.environ.get(options.bff_token_env)
        if not token or not os.environ.get(options.gateway_token_env):
            raise ApiError("both control and inference credentials must be supplied through environment variables")
        config = json.loads(Path(options.config).read_text())
        if not isinstance(config, dict):
            raise ApiError("authorized configuration must be a JSON object")
        # Reject malformed workloads before reserving any GPUs.
        benchmark.parse_args([
            "--workload-file", options.workload_file, "--model", options.model,
            "--base-url", options.gateway_url, "--requests", str(options.requests),
            "--concurrency", str(options.concurrency),
        ])
        api = Api(options.bff_url, token)
        cycle = 0
        while options.cycles == 0 or cycle < options.cycles:
            request = {**config, "owner": str(uuid.uuid4())}
            job = api.request(f"/api/v2/models/{quote(options.model, safe='')}/autotune", request)
            # Print a recovery identifier, never credentials or supplied workload text.
            print(canonical({"experiment": job["id"], "phase": job["phase"]}), flush=True)
            result = Controller(api, job, options).run()
            print(canonical({"experiment": result["id"], "phase": result["phase"], "reason": result["reason"]}), flush=True)
            if result["phase"] != "completed":
                return 1
            cycle += 1
            if options.cycles == 0 or cycle < options.cycles:
                time.sleep(options.interval)
        return 0
    except KeyboardInterrupt:
        print("optimization interrupted; BFF will reconcile rollback and cleanup", file=sys.stderr)
        return 130
    except (ApiError, OSError, ValueError, KeyError, TypeError):
        print("optimization stopped; inspect its BFF status and evidence", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
