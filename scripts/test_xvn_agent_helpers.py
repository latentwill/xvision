from __future__ import annotations

import importlib.util
import io
import pathlib
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch


SCRIPTS_DIR = pathlib.Path(__file__).parent
sys.path.insert(0, str(SCRIPTS_DIR))


def load_script(name: str):
    path = SCRIPTS_DIR / name
    module_name = name.removesuffix(".py").replace("-", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class AgentHelperTests(unittest.TestCase):
    def test_filter_attach_dry_runs_without_yes(self) -> None:
        filter_lab = load_script("xvn_filter_lab.py")
        with tempfile.NamedTemporaryFile("w", suffix=".json") as f:
            f.write(
                """
{
  "display_name": "RSI gate",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "conditions": {"lhs": "rsi_14", "op": "below", "rhs": 30}
}
"""
            )
            f.flush()

            stdout = io.StringIO()
            with patch.object(filter_lab.XvnApi, "patch") as patch_method:
                with redirect_stdout(stdout):
                    exit_code = filter_lab.main(["attach", "strategy_1", f.name])

        self.assertEqual(exit_code, 0)
        patch_method.assert_not_called()

    def test_filter_attach_uses_strategy_patch_route(self) -> None:
        filter_lab = load_script("xvn_filter_lab.py")

        class Resp:
            status = 200
            payload = {"id": "strategy_1"}

        with tempfile.NamedTemporaryFile("w", suffix=".json") as f:
            f.write(
                """
{
  "filter": {
    "display_name": "RSI gate",
    "asset_scope": ["BTC/USD"],
    "timeframe": "1h",
    "conditions": {"lhs": "rsi_14", "op": "below", "rhs": 30}
  }
}
"""
            )
            f.flush()

            stdout = io.StringIO()
            with patch.object(filter_lab.XvnApi, "patch", return_value=Resp()) as patch_method:
                with redirect_stdout(stdout):
                    exit_code = filter_lab.main(["attach", "strategy_1", f.name, "--yes"])

        self.assertEqual(exit_code, 0)
        patch_method.assert_called_once()
        path, body = patch_method.call_args.args
        self.assertEqual(path, "/api/strategy/strategy_1")
        self.assertEqual(body["filter"]["display_name"], "RSI gate")

    def test_eval_summary_uses_strategy_id_before_agent_id(self) -> None:
        harness = load_script("xvn_eval_harness.py")
        summary = harness.summarize_export(
            {
                "run": {
                    "id": "run_1",
                    "status": "completed",
                    "strategy_id": "strategy_1",
                    "agent_id": "legacy_agent",
                    "scenario_id": "scenario_1",
                },
                "decisions": [
                    {"decision": {"action": "hold"}},
                    {"action": "buy"},
                    {"decision": {"action": "hold"}},
                ],
                "strategy": {"id": "strategy_1"},
                "agents": [{"id": "agent_1"}],
            }
        )

        self.assertEqual(summary["strategy_id"], "strategy_1")
        self.assertEqual(summary["actions"], {"hold": 2, "buy": 1})
        self.assertTrue(summary["export_integrity"]["strategy_present"])


if __name__ == "__main__":
    unittest.main()
