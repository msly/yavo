"""Focused promotion and frozen-history integrity controls; no C backend required."""
from pathlib import Path
import copy
import json
import sys
import shutil
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/ci'))
import selfhost_diagnostics as acceptance


class DiagnosticPromotions(unittest.TestCase):
    def test_three_promotions_preserve_every_other_case_and_source(self):
        frozen, oracle = acceptance.load_frozen({})
        effective, _, record = acceptance.load_effective({})
        self.assertEqual(len(frozen['cases']), 98)
        self.assertEqual(len(effective['cases']), 98)
        self.assertEqual(set(record['ids']), acceptance.PROMOTED_IDS)
        for before, after in zip(frozen['cases'], effective['cases']):
            self.assertEqual(before['id'], after['id'])
            if before['id'] not in acceptance.PROMOTED_IDS:
                self.assertEqual(before, after)
                continue
            self.assertEqual(before['diagnostic']['code'], 'U0001')
            self.assertEqual(before['expected']['exit'], 1)
            for key in ('files', 'loaded', 'arguments', 'platforms', 'input', 'kind'):
                self.assertEqual(before[key], after[key])
            self.assertEqual(after['expected']['exit'], 0)
            self.assertEqual(after['expected']['document']['error_count'], 0)
            self.assertEqual(after['expected']['document']['diagnostics'], [])
            self.assertEqual(oracle.legacy(after), {'exit': 0, 'stdout': '', 'stderr': ''})

    def test_invalid_promotion_records_are_rejected(self):
        manifest, oracle = acceptance.load_frozen({})
        original = json.loads(acceptance.PROMOTIONS.read_bytes())
        real_read = Path.read_bytes
        changes = {
            'extra_case': lambda r: r['promotions'].append(copy.deepcopy(r['promotions'][0])),
            'wrong_before_hash': lambda r: r['promotions'][0].__setitem__('before_case_sha256', '0' * 64),
            'wrong_source_identity': lambda r: r['promotions'][0]['files'][0].__setitem__('sha256', '0' * 64),
            'changed_old_expectation': lambda r: r['promotions'][0]['before_expected'].__setitem__('exit', 0),
            'failure_instead_of_success': lambda r: r['promotions'][0]['after_expected'].__setitem__('exit', 1),
            'hidden_text_output': lambda r: r['promotions'][0]['after_legacy_text'].__setitem__('stdout', 'unexpected'),
        }
        for name, change in changes.items():
            with self.subTest(name=name):
                changed = copy.deepcopy(original)
                change(changed)
                data = json.dumps(changed).encode()
                def read(path):
                    return data if path == acceptance.PROMOTIONS else real_read(path)
                with patch.object(Path, 'read_bytes', read), self.assertRaises(ValueError):
                    acceptance.apply_promotions(manifest, oracle, {})

    def test_frozen_manifest_and_source_mutations_are_rejected(self):
        for relative in ('cases.json', 'fixtures/unsupported_hex/main.yv'):
            with self.subTest(path=relative), tempfile.TemporaryDirectory(prefix='diagnostic-promotion-') as scratch:
                copied = Path(scratch) / 'fixtures'
                shutil.copytree(acceptance.FIXTURES, copied)
                changed = copied / relative
                changed.write_bytes(changed.read_bytes() + b' ')
                with patch.object(acceptance, 'FIXTURES', copied), self.assertRaises(ValueError):
                    acceptance.load_effective({})


if __name__ == '__main__':
    unittest.main()
