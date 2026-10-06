"""Prevent accidental reintroduction of billable automatic Actions runs."""
import re
import unittest
from pathlib import Path


class ManualWorkflowsTests(unittest.TestCase):
    def test_every_workflow_has_only_a_manual_trigger(self):
        workflows = Path(__file__).resolve().parents[1] / ".github" / "workflows"
        paths = sorted([*workflows.glob("*.yml"), *workflows.glob("*.yaml")])
        self.assertTrue(paths)
        for path in paths:
            with self.subTest(workflow=path.name):
                match = re.search(r"^on:\s*\n((?:[ \t].*\n|\n)*)", path.read_text(), re.MULTILINE)
                self.assertIsNotNone(match, "workflow must declare an explicit trigger block")
                triggers = re.findall(r"^  ([a-z_]+):", match.group(1), re.MULTILINE)
                self.assertEqual(triggers, ["workflow_dispatch"])


if __name__ == "__main__":
    unittest.main()
