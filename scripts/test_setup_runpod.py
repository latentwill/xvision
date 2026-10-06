"""Exercise only the provider setup stage; no GPU, builds, or API calls."""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


class RunpodProviderSetupTests(unittest.TestCase):
    def run_setup(self, intern=None):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "scripts").mkdir()
            script = root / "scripts" / "setup_runpod.sh"
            shutil.copyfile(Path(__file__).with_name("setup_runpod.sh"), script)
            env = {k: v for k, v in os.environ.items() if k not in ("INTERN", "ASSUME_YES")}
            env.update(ONLY="intern", ASSUME_YES="1")
            if intern is not None:
                env["INTERN"] = intern
            result = subprocess.run(["bash", str(script)], env=env, capture_output=True, text=True, timeout=5)
            env_file = root / ".env.local"
            return result, env_file.read_text() if env_file.exists() else ""

    def test_groq_override_fails_without_persisting_credentials(self):
        result, config = self.run_setup("groq")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Groq is disabled", result.stderr)
        self.assertEqual(config, "")

    def test_unattended_setup_does_not_select_a_paid_provider(self):
        result, config = self.run_setup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("selected: skip", result.stdout)
        self.assertEqual(config, "")


if __name__ == "__main__":
    unittest.main()
