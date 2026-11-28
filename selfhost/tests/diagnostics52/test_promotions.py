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
        effective, record = acceptance.apply_promotions(frozen, oracle, {})
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

    def test_layered_promotions_preserve_inventory_sources_and_first_layer(self):
        frozen, oracle = acceptance.load_frozen({})
        first, first_record = acceptance.apply_promotions(frozen, oracle, {})
        bound = {}
        effective, _, record = acceptance.load_effective(bound)
        self.assertEqual(len(effective['cases']), 98)
        self.assertEqual(record['schema'], 'yavo-diagnostic-promotion-chain/1')
        self.assertEqual([layer['batch'] for layer in record['layers']], [52, 53])
        self.assertEqual(record['layers'][0], first_record)
        self.assertEqual(set(record['ids']), acceptance.PROMOTED_IDS | acceptance.PROMOTED_IDS53)
        for path in (acceptance.PROMOTIONS, acceptance.PROMOTIONS53):
            self.assertEqual(bound[str(path.resolve())], acceptance.digest(path))
        for before, after in zip(first['cases'], effective['cases']):
            if before['id'] not in acceptance.PROMOTED_IDS53:
                self.assertEqual(before, after)
                continue
            for key in ('files', 'loaded', 'arguments', 'platforms', 'input', 'kind'):
                self.assertEqual(before[key], after[key])
            self.assertEqual(before['expected']['exit'], 1)
            self.assertEqual(after['expected']['exit'], 0)
            self.assertEqual(after['expected']['document']['diagnostics'], [])
            self.assertEqual(oracle.legacy(after), {'exit': 0, 'stdout': '', 'stderr': ''})

    def test_second_layer_tampering_is_rejected(self):
        original = json.loads(acceptance.PROMOTIONS53.read_bytes())
        real_read = Path.read_bytes
        changes = {
            'overlapping_first_layer': lambda r: r['promotions'][0].__setitem__('id', 'unsupported_hex'),
            'wrong_before_hash': lambda r: r['promotions'][0].__setitem__('before_case_sha256', '0' * 64),
            'changed_source': lambda r: r['promotions'][0]['files'][0].__setitem__('sha256', '0' * 64),
            'wrong_success': lambda r: r['promotions'][0]['after_expected']['document'].__setitem__('error_count', 1),
            'hidden_text_output': lambda r: r['promotions'][0]['after_legacy_text'].__setitem__('stdout', 'unexpected'),
        }
        for name, change in changes.items():
            with self.subTest(name=name):
                changed = copy.deepcopy(original)
                change(changed)
                data = json.dumps(changed).encode()
                def read(path):
                    return data if path == acceptance.PROMOTIONS53 else real_read(path)
                with patch.object(Path, 'read_bytes', read), self.assertRaises(ValueError):
                    acceptance.load_effective({})

    def test_versioned_c_product_keeps_frozen_comparator_and_rejects_wrong_bytes(self):
        bound = {}
        manifest, oracle = acceptance.load_supplement(bound)
        self.assertEqual(len(manifest['cases']), 8)
        self.assertEqual(acceptance.PRODUCT53_C.read_bytes(), acceptance.legacy_product53())
        self.assertEqual(oracle.ROOT, acceptance.PRODUCT53_C.parent)
        for path in (acceptance.PRODUCT53, acceptance.PRODUCT53_C):
            self.assertEqual(bound[str(path.resolve())], acceptance.digest(path))
        case = next(c for c in manifest['cases'] if c['id'] == 'emit_output_named_json')
        with tempfile.TemporaryDirectory(prefix='legacy-c-product-') as scratch:
            target = Path(scratch) / case['output']
            target.write_bytes(acceptance.PRODUCT53_C.read_bytes())
            self.assertEqual(oracle.check(case, 0, b'', b'', scratch), [])
            target.write_bytes(target.read_bytes() + b' ')
            self.assertTrue(oracle.check(case, 0, b'', b'', scratch))

    def test_product_and_forged_matching_identity_still_require_authored_transform(self):
        product = acceptance.PRODUCT53_C.read_bytes() + b' '
        record = json.loads(acceptance.PRODUCT53.read_bytes())
        record['replacement'].update(bytes=len(product), sha256=acceptance.execution.sha(product))
        with tempfile.TemporaryDirectory(prefix='legacy-c-forged-') as scratch:
            changed_c = Path(scratch) / 'expected.c'
            changed_record = Path(scratch) / 'legacy-product.json'
            changed_c.write_bytes(product)
            changed_record.write_text(json.dumps(record))
            with (patch.object(acceptance, 'PRODUCT53_C', changed_c),
                  patch.object(acceptance, 'PRODUCT53', changed_record), self.assertRaises(ValueError)):
                acceptance.load_supplement({})

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
