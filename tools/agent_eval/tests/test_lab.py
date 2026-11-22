#!/usr/bin/env python3
"""R6 protocol regression tests. All receipts, models and compilers are SYNTHETIC.

The tests only create disposable temporary repositories. They never qualify an
executor, access a provider, or create runs inside the real experiment packet.
Set YAVO_TEST_LAB to select the lab.pl under review; repository discovery is used
when this file is installed at tools/agent_eval/tests/test_lab.py.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

LAB_SOURCE = Path(os.environ.get(
    'YAVO_TEST_LAB',
    str(Path(__file__).resolve().parents[3] / 'experiments/docs-ab-2026-11-22-r6/tools/lab.pl'),
))
EXPERIMENT = 'docs-ab-2026-11-22-r6'


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def put(path: Path, value: bytes | str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(value.encode('utf-8') if isinstance(value, str) else value)


def dump(path: Path, value: object) -> None:
    put(path, json.dumps(value, ensure_ascii=False, indent=2) + '\n')


class SyntheticLab:
    def __init__(self, source: Path):
        self.temporary = tempfile.TemporaryDirectory(prefix='yavo-synthetic-lab-')
        self.root = Path(self.temporary.name)
        self.base = self.root / 'experiments' / EXPERIMENT
        self.lab = self.base / 'tools/lab.pl'
        self.run_dir = self.base / 'runs/full-a'
        self.frozen_path = self.base / 'evidence/frozen.json'
        self.qualification_path = self.base / 'evidence/launch-qualification.json'
        put(self.lab, source.read_bytes())
        self.compiler = self.root / 'compiler/target/release/yavo'
        # This fixture copies no language implementation. It merely emits an
        # executable with a fixed output so build/public/submit states can run.
        put(self.compiler, f'''#!{sys.executable}
import pathlib, sys
out = pathlib.Path(sys.argv[sys.argv.index("-o") + 1])
out.write_text("#!/bin/sh\\nprintf 'ok\\\\n'\\n")
out.chmod(0o700)
''')
        self.compiler.chmod(0o700)
        put(self.root / 'compiler/runtime/yavo_rt.h', '/* SYNTHETIC TEST RUNTIME */\n')
        put(self.root / 'compiler/toolchain.lock', 'SYNTHETIC TEST LOCK\n')
        self.executor = self.root / 'tools/agent_eval/executor.py'
        put(self.executor, '# SYNTHETIC TEST EXECUTOR; DOES NOT CALL PROVIDERS\n')
        self.toolchain = self.root / 'synthetic-toolchain'
        put(self.toolchain, 'SYNTHETIC TOOLCHAIN IDENTITY\n')
        self.guide = ('x' * 8191 + '你' + '🙂' * 2100 + '\nEND\n').encode('utf-8')
        put(self.base / 'docs/full.md', self.guide)
        put(self.base / 'docs/compact.md', 'synthetic compact guide\n')
        put(self.base / 'docs/index.md', 'synthetic API 索引\n')
        dump(self.base / 'documents.json', {
            'page_bytes': 8192,
            'documents': {
                'guide': {'path': {'full': 'docs/full.md', 'compact': 'docs/compact.md'}, 'category': 'read_documentation'},
                'index': {'path': 'docs/index.md', 'category': 'read_documentation'},
            },
        })
        case = {'id': 'T01-public-01', 'visibility': 'public', 'stdin': '',
                'stdout': 'ok\n', 'stderr': '', 'exit_code': 0, 'args': []}
        packet = {'tasks': [{'id': 'T01', 'cases': [case]}]}
        dump(self.base / 'public/cases.json', packet)
        dump(self.base / 'judge/cases.json', packet)
        dump(self.base / 'evidence/compiler-config.json', {
            'yavo': {'argv': [str(self.compiler), 'build', '{entry}', '-o', '{output}'], 'env': {}},
        })
        identities = {str(p): sha(p.read_bytes()) for p in self.root.rglob('*')
                      if p.is_file() and p != self.toolchain}
        self.frozen = {
            'kind': 'yavo.docs_ab.freeze', 'experiment_id': EXPERIMENT,
            'compiler_revision': 'SYNTHETIC-TEST-REVISION',
            'identity_files': identities,
            'toolchain_files': {str(self.toolchain): sha(self.toolchain.read_bytes())},
            'executor_sha256': sha(self.executor.read_bytes()),
            'model_settings_sha256': sha(b'SYNTHETIC TEST SETTINGS'),
        }
        self.qualification = {
            'synthetic_test_fixture': True,
            'status': 'qualified', 'kind': 'yavo.executor.qualification', 'experiment_id': EXPERIMENT,
            'executor_sha256': self.frozen['executor_sha256'],
            'model_settings_sha256': self.frozen['model_settings_sha256'],
            'observed': {'provider_requests': 2, 'provider_responses': 2, 'qualifying_tool_calls': 1,
                         'usage_complete': True, 'tool_accounting_complete': True,
                         'per_request_evidence': True, 'reported_models': ['SYNTHETIC-TEST-MODEL']},
        }
        self.refresh_freeze()

    def refresh_freeze(self):
        dump(self.frozen_path, self.frozen)
        self.qualification['freeze_sha256'] = sha(self.frozen_path.read_bytes())
        dump(self.qualification_path, self.qualification)

    def invoke(self, *args: str, source: bytes = b'') -> subprocess.CompletedProcess:
        environment = os.environ.copy()
        environment['YAVO_QUALIFICATION_RECEIPT'] = str(self.qualification_path)
        return subprocess.run(['perl', str(self.lab), 'full-a', *args], input=source,
                              cwd=self.root, env=environment, capture_output=True, timeout=10)

    def metadata(self):
        return json.loads((self.run_dir / 'run.json').read_bytes())

    def close(self):
        self.temporary.cleanup()


class LabProtocolTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(shutil.which('perl'), 'Perl is required for protocol tests')
        self.assertTrue(LAB_SOURCE.is_file(), f'lab under test missing: {LAB_SOURCE}')
        self.lab = SyntheticLab(LAB_SOURCE)
        self.addCleanup(self.lab.close)

    def ok(self, *args, source=b''):
        result = self.lab.invoke(*args, source=source)
        self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', 'replace'))
        return json.loads(result.stdout)

    def rejected(self, *args, message, source=b''):
        result = self.lab.invoke(*args, source=source)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(message, result.stderr.decode('utf-8', 'replace'))
        return result

    def read_initial(self):
        for name in ('guide', 'index'):
            first = self.ok('doc', name, '1')
            for page in range(2, first['total_pages'] + 1):
                self.ok('doc', name, str(page))

    def ready_build(self):
        self.ok('init')
        self.read_initial()
        self.ok('write', 'T01', 'main.yv', source=b'SYNTHETIC FIRST SOURCE\n')
        self.ok('build', 'T01')
        self.ok('public', 'T01')

    def test_missing_receipt_creates_no_run(self):
        self.lab.qualification_path.unlink()
        self.rejected('init', message='missing executor qualification')
        self.assertFalse(self.lab.run_dir.exists())

    def test_blocked_receipt_creates_no_run(self):
        self.lab.qualification['status'] = 'blocked'
        dump(self.lab.qualification_path, self.lab.qualification)
        self.rejected('init', message='executor is not qualified')
        self.assertFalse(self.lab.run_dir.exists())

    def test_empty_identity_inventory_rejected(self):
        self.lab.frozen['identity_files'] = {}
        self.lab.refresh_freeze()
        self.rejected('init', message='missing identity inventory')
        self.assertFalse(self.lab.run_dir.exists())

    def test_missing_required_identity_rejected(self):
        del self.lab.frozen['identity_files'][str(self.lab.compiler)]
        self.lab.refresh_freeze()
        self.rejected('init', message='missing required identity')
        self.assertFalse(self.lab.run_dir.exists())

    def test_missing_toolchain_inventory_rejected(self):
        del self.lab.frozen['toolchain_files']
        self.lab.refresh_freeze()
        self.rejected('init', message='missing toolchain inventory')
        self.assertFalse(self.lab.run_dir.exists())

    def test_qualification_settings_and_executor_must_match(self):
        for field, message in [('model_settings_sha256', 'settings mismatch'), ('executor_sha256', 'executor mismatch')]:
            with self.subTest(field=field):
                original = self.lab.qualification[field]
                self.lab.qualification[field] = '0' * 64
                dump(self.lab.qualification_path, self.lab.qualification)
                self.rejected('init', message=message)
                self.assertFalse(self.lab.run_dir.exists())
                self.lab.qualification[field] = original

    def test_no_tool_roundtrip_receipt_cannot_launch(self):
        self.lab.qualification['observed']['qualifying_tool_calls'] = 0
        dump(self.lab.qualification_path, self.lab.qualification)
        self.rejected('init', message='missing live tool round trip')
        self.assertFalse(self.lab.run_dir.exists())

    def test_utf8_pages_and_argument_strings_preserved(self):
        self.ok('init')
        page = self.ok('doc', 'guide', '1')
        pages = [page]
        for number in range(2, page['total_pages'] + 1):
            pages.append(self.ok('doc', 'guide', str(number)))
        reconstructed = b''
        for number, page in enumerate(pages, 1):
            content = page['content'].encode('utf-8')
            self.assertLessEqual(len(content), 8192)
            self.assertEqual(page['bytes'], len(content))
            reconstructed += content
            directory = self.lab.run_dir / f'operations/{number:04d}'
            request = json.loads((directory / 'request.json').read_bytes())
            event = json.loads((directory / 'event.json').read_bytes())
            self.assertEqual(request['arguments'], ['guide', str(number)])
            self.assertEqual(event['arguments'], ['guide', str(number)])
            self.assertEqual(event['document_page_sha256'], sha(content))
            self.assertEqual(event['document_bytes_returned'], len(content))
        self.assertEqual(reconstructed, self.lab.guide)

    def test_unread_guide_pages_block_first_build(self):
        self.ok('init')
        self.ok('write', 'T01', 'main.yv', source=b'SYNTHETIC FIRST SOURCE\n')
        self.rejected('build', 'T01', message='read all initial guide and index pages')
        self.ok('doc', 'guide', '1')
        self.ok('doc', 'index', '1')
        self.rejected('build', 'T01', message='guide page 2')
        self.assertEqual(self.lab.metadata()['tasks']['T01']['builds'], 0)
        self.assertFalse((self.lab.run_dir / 'tasks/T01/snapshots/0001').exists())

    def test_missing_entry_does_not_consume_first_build_or_allow_submit(self):
        self.ok('init')
        self.read_initial()
        self.rejected('build', 'T01', message='missing entry source')
        self.assertEqual(self.lab.metadata()['tasks']['T01']['builds'], 0)
        self.assertFalse((self.lab.run_dir / 'tasks/T01/snapshots/0001').exists())
        self.rejected('submit', 'T01', message='at least one build attempt required')
        source = b'SYNTHETIC FIRST REAL COMPILER INPUT\n'
        self.ok('write', 'T01', 'main.yv', source=source)
        build = self.ok('build', 'T01')
        self.assertEqual(build['build'], 1)
        self.assertEqual((self.lab.run_dir / 'tasks/T01/snapshots/0001/main.yv').read_bytes(), source)
        self.assertTrue(self.lab.metadata()['tasks']['T01']['first_build_passed'])
        self.rejected('submit', 'T01', message='run public samples')
        self.ok('public', 'T01')
        self.ok('submit', 'T01')

    def test_direct_source_drift_cannot_submit(self):
        self.ready_build()
        put(self.lab.run_dir / 'tasks/T01/work/main.yv', 'SYNTHETIC UNBUILT CHANGE\n')
        self.rejected('submit', 'T01', message='source bytes differ from latest snapshot')
        self.assertFalse((self.lab.run_dir / 'tasks/T01/final').exists())

    def test_qualification_receipt_drift_rejected(self):
        self.ok('init')
        put(self.lab.qualification_path, self.lab.qualification_path.read_bytes() + b'\n')
        self.rejected('doc', 'guide', '1', message='qualification receipt changed')
        self.assertEqual(self.lab.metadata()['operations'], 0)

    def test_freeze_receipt_drift_rejected(self):
        self.ok('init')
        put(self.lab.frozen_path, self.lab.frozen_path.read_bytes() + b'\n')
        self.rejected('doc', 'guide', '1', message='experiment freeze changed')
        self.assertEqual(self.lab.metadata()['operations'], 0)

    def test_frozen_document_drift_rejected_before_read(self):
        self.ok('init')
        put(self.lab.base / 'docs/full.md', 'SYNTHETIC DOCUMENT CHANGE\n')
        self.rejected('doc', 'guide', '1', message='frozen identity changed')
        self.assertEqual(self.lab.metadata()['operations'], 0)


if __name__ == '__main__':
    unittest.main()
