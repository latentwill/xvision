import importlib.util
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("generate_strategy_template_files.py")
SPEC = importlib.util.spec_from_file_location("generate_strategy_template_files", SCRIPT_PATH)
generator = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(generator)


class GenerateStrategyTemplateFilesTests(unittest.TestCase):
    def test_qualified_parameter_heading_is_canonicalized(self) -> None:
        raw = """# Split Variant

## Thesis
Works in split regimes.

## Parameters (split: upper-rule / lower-rule)
| Param | Default | Range |
| --- | --- | --- |
| lookback | 20 | 10-60 |
"""

        _title, _metadata, sections = generator.parse_sections(raw)

        self.assertIn("parameters", sections)
        self.assertNotIn("parameters_split_upper_rule_lower_rule", sections)
        self.assertEqual(
            generator.parse_parameters(sections["parameters"]),
            [{"name": "lookback", "default": "20", "range": "10-60"}],
        )


if __name__ == "__main__":
    unittest.main()
