"""Autotuning contracts use local streams, not claims of GPU improvement."""

import argparse
import contextlib
import io
import json
from pathlib import Path
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer
from unittest.mock import Mock, patch

import autotune
from test_run_benchmark import Handler


def fixture(directory, base_url):
    workload = Path(directory) / "workload.json"
    workload.write_text(json.dumps({
        "id": "test", "class": "custom", "prompt": "fixture", "max_tokens": 8,
    }))
    options = argparse.Namespace(
        workload_file=str(workload), requests=16, concurrency=8,
        gateway_url=base_url, seed=42, request_timeout=10,
        artifacts_dir=directory, bff_url="http://127.0.0.1:1",
        gateway_token_env="NEBULA_AUTOTUNE_GATEWAY_TOKEN",
        startup_timeout=1,
    )
    parameters = {"max_num_seqs": 16, "max_num_batched_tokens": 2048}
    job = {
        "id": "job", "owner": "owner", "epoch": 1, "phase": "ready",
        "model_uid": "production", "experiment_uid": "trial",
        "platform": "nvidia-cuda", "gpu_name": "Test GPU", "parameters": parameters,
        "trial": {"version": 2, "image_id": "image@sha256:fixture",
                  "replica_specs": [{"node_id": "spare", "gpu_indices": [0]}]},
        "baseline": {"version": 1, "replicas": 1},
        "request": {
            "engine_version": "0.1", "candidates": [parameters],
            "objective": {"ttft_p95_ms_max": 1000, "tpot_p95_ms_max": 1000,
                          "e2e_p95_ms_max": 2000, "min_improvement": 0.1,
                          "max_regression": 0.05, "min_requests": 16},
        },
    }
    return job, options


class AutotuneTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.server.lock = threading.Lock()
        cls.server.active = cls.server.peak = 0
        cls.server.bodies = []
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.server.mode = "normal"
        self.server.bodies = []
        job, options = fixture(directory.name, f"http://127.0.0.1:{self.server.server_port}/v1")
        self.api = Mock()
        self.api.request.return_value = job
        self.controller = autotune.Controller(self.api, job, options)

    def test_full_streaming_plan_warms_before_evidence_is_recorded(self):
        with patch.dict("os.environ", {"NEBULA_AUTOTUNE_GATEWAY_TOKEN": "fixture"}):
            self.controller.measure(0, 0)
        self.assertEqual(len(self.server.bodies), 32)
        run = self.api.request.call_args_list[0].args[1]
        self.assertEqual(run["profile_key"]["model_name"], "trial")
        self.assertEqual(run["status"], "succeeded")
        self.assertEqual(run["evidence"]["successful_requests"], 16)
        self.assertEqual(run["evidence"]["cache_state_source"], "full_plan_warmup")
        self.assertEqual(run["evidence"]["comparison_group"], "job")
        self.assertEqual(run["evidence"]["abba_position"], 0)
        self.assertTrue(run["evidence"]["trace_sha256"])
        self.assertTrue(run["evidence"]["samples_sha256"])
        self.assertEqual(self.api.request.call_args.args[1]["operation"]["action"], "record")
        manifest = next(Path(self.controller.options.artifacts_dir).rglob("manifest.json"))
        self.assertEqual(json.loads(manifest.read_text())["run"]["run_id"], run["run_id"])

    def test_failed_warmup_never_becomes_warm_evidence(self):
        self.server.mode = "http_error"
        with patch.dict("os.environ", {"NEBULA_AUTOTUNE_GATEWAY_TOKEN": "fixture"}):
            with self.assertRaises(autotune.WarmupError):
                self.controller.measure(0, 1)
        self.api.request.assert_not_called()
        self.assertTrue(list(Path(self.controller.options.artifacts_dir).rglob("failed_warmup.json")))

    def test_dead_lease_fences_commands_before_any_http_write(self):
        self.controller.lease_lost.set()
        with self.assertRaises(autotune.ApiError):
            self.controller.command("promote", candidate=0)
        self.api.request.assert_not_called()

    def test_production_readiness_accepts_rust_null_fields_but_not_other_parameters(self):
        controller = self.controller
        config = {
            "max_num_seqs": 16, "max_num_batched_tokens": 2048,
            "tensor_parallel_size": None, "gpu_memory_utilization": None,
            "max_model_len": None, "enable_chunked_prefill": None,
        }
        detail = {
            "deployment": {"version": 2, "config_overrides": config},
            "placement": {"version": 7, "assignments": [{"replica_id": 0}]},
            "endpoints": [{"replica_id": 0, "status": "ready", "plan_version": 7}],
        }
        self.api.request.return_value = detail
        controller.wait_for_endpoints(production=True)
        detail["deployment"]["config_overrides"]["max_num_seqs"] = 17
        with patch.object(controller.closed, "wait"), \
                patch.object(autotune.time, "monotonic", side_effect=[0, 0, 2]):
            with self.assertRaises(autotune.ApiError):
                controller.wait_for_endpoints(production=True)

    def test_candidate_requires_repetitions_and_strict_slo(self):
        def row(candidate, position, goodput):
            return {
                "candidate": candidate, "production": False,
                "run": {
                    "status": "succeeded", "error_rate": 0.0,
                    "ttft_p95_ms": 10, "tpot_p95_ms": 2,
                    "evidence": {"synthetic": False, "schema_version": 2,
                                 "request_count": 16, "successful_requests": 16,
                                 "goodput_rps": goodput, "e2e_p95_ms": 50,
                                 "round": 1, "abba_position": position},
                },
            }
        job = self.controller.job
        job["measurements"] = [row(None, 0, 10), row(0, 1, 12), row(0, 2, 12), row(None, 3, 10)]
        self.assertEqual(autotune.eligible_candidates(job), [0])
        job["measurements"][2]["run"]["ttft_p95_ms"] = 2000
        self.assertEqual(autotune.eligible_candidates(job), [])
        job["measurements"].pop()
        self.assertEqual(autotune.eligible_candidates(job), [])

    def test_cli_rejects_unbounded_request_budgets(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            autotune.parse_args([
                "--model", "model", "--config", "c", "--workload-file", "w",
                "--bff-url", "http://127.0.0.1:3000", "--gateway-url", "http://127.0.0.1:8080/v1",
                "--concurrency", "8", "--artifacts-dir", "/tmp/unused-autotune-artifacts",
                "--requests", "100001",
            ])


if __name__ == "__main__":
    unittest.main()
