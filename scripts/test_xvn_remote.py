from __future__ import annotations

import importlib.util
import io
import pathlib
import sys
import unittest
from contextlib import redirect_stderr
from unittest.mock import patch


SCRIPT_PATH = pathlib.Path(__file__).with_name("xvn-remote.py")


def load_remote_module():
    spec = importlib.util.spec_from_file_location("xvn_remote", SCRIPT_PATH)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class RemoteCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.remote = load_remote_module()

    def test_job_ids_are_encoded_as_one_path_segment(self) -> None:
        calls: list[tuple[str, str]] = []

        def fake_request_json(method: str, url: str, body=None):
            calls.append((method, url))
            return self.remote.HttpResult(200, {})

        with patch.object(self.remote, "request_json", fake_request_json):
            self.remote.get_job("https://host/", "job/a b")
            self.remote.get_output("https://host/", "job/a b")
            self.remote.cancel_job("https://host/", "job/a b")

        self.assertEqual(
            calls,
            [
                ("GET", "https://host/api/cli/jobs/job%2Fa%20b"),
                ("GET", "https://host/api/cli/jobs/job%2Fa%20b/output"),
                ("POST", "https://host/api/cli/jobs/job%2Fa%20b/cancel"),
            ],
        )

    def test_exec_reports_malformed_submit_without_traceback(self) -> None:
        def fake_request_json(method: str, url: str, body=None):
            self.assertEqual(method, "POST")
            self.assertEqual(url, "https://host/api/cli/jobs")
            return self.remote.HttpResult(200, {})

        stderr = io.StringIO()
        with patch.object(self.remote, "request_json", fake_request_json):
            with redirect_stderr(stderr):
                exit_code = self.remote.main(
                    ["--url", "https://host", "exec", "eval", "list"]
                )

        self.assertEqual(exit_code, 1)
        self.assertIn("missing job_id", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
