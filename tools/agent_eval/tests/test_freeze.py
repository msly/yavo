#!/usr/bin/env python3
"""Freeze/driver integration using disposable, explicitly synthetic host tools.

No API keys, providers, real packet freeze, or participant runs are used. The
neutral lab probe is the only lab operation after the synthetic runtime freeze.
"""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO_SOURCE = Path(os.environ.get('YAVO_TEST_REPO', Path(__file__).resolve().parents[3]))
EXECUTOR_SOURCE = Path(os.environ.get('YAVO_TEST_EXECUTOR', REPO_SOURCE / 'tools/agent_eval/executor.py'))
PACKET = Path('experiments/docs-ab-2026-11-22-r6')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def put(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw.encode('utf-8') if isinstance(raw, str) else raw)


def dump(path, value):
    put(path, json.dumps(value, indent=2, sort_keys=True) + '\n')


class SyntheticFreeze:
    def __init__(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='yavo-synthetic-freeze-')
        self.root = Path(self.temporary.name)
        self.base = self.root / PACKET
        self.home = self.root / 'synthetic-yavo-home'
        self.zig = self.home / 'toolchains/synthetic/zig'
        self.executor_path = self.root / 'tools/agent_eval/executor.py'
        self.config_path = self.base / 'EXECUTOR_CONFIG.json'
        self.frozen_path = self.base / 'evidence/frozen.json'
        self.compiler_config_path = self.base / 'evidence/compiler-config.json'
        self.environment = {key: os.environ[key] for key in ('PATH', 'HOME', 'LANG') if key in os.environ}
        self.environment['YAVO_HOME'] = str(self.home)
        for name in ('freeze.py', 'lab.pl'):
            put(self.base / 'tools' / name, (REPO_SOURCE / PACKET / 'tools' / name).read_bytes())
        put(self.executor_path, EXECUTOR_SOURCE.read_bytes())
        put(self.zig, f'''#!{sys.executable}
import sys
if sys.argv[1:] != ["version"]:
    sys.exit(2)
print("0.17.1")
''')
        self.zig.chmod(0o700)
        compiler = self.root / 'compiler/target/release/yavo'
        put(compiler, f'''#!{sys.executable}
import os, pathlib, sys
if sys.argv[1:] != ["toolchain", "status"]:
    sys.exit(2)
backend = pathlib.Path(os.environ["YAVO_HOME"]) / "toolchains/synthetic/zig"
print(f"C toolchain: zig 0.17.1 ({{backend}})")
''')
        compiler.chmod(0o700)
        put(self.root / 'compiler/runtime/yavo_rt.h', '/* SYNTHETIC TEST RUNTIME */\n')
        put(self.root / 'compiler/toolchain.lock', 'SYNTHETIC TEST TOOLCHAIN LOCK\n')
        self.neutral = b'SYNTHETIC neutral metering probe, no task information.\n'
        put(self.base / 'METER_PROBE.txt', self.neutral)
        dump(self.base / 'documents.json', {'page_bytes': 8192, 'documents': {}})
        for directory in ('public', 'judge'):
            dump(self.base / directory / 'cases.json', {'tasks': [], 'synthetic_test_fixture': True})
        self.config = {
            'schema_version': 1, 'experiment_id': 'docs-ab-2026-11-22-r6',
            'freeze': {'path': 'evidence/frozen.json', 'sha256': None},
            'provider': {'endpoint': 'https://api.openai.com/v1/responses',
                         'requested_model': 'SYNTHETIC-NO-NETWORK',
                         'expected_reported_model': 'SYNTHETIC-NO-NETWORK',
                         'parameters': {'max_output_tokens': 64}},
            'lab': {'program': 'perl', 'script': 'tools/lab.pl', 'timeout_seconds': 120, 'output_bytes': 1048576},
            'limits': {'max_provider_requests': 2, 'provider_timeout_seconds': 10, 'max_response_bytes': 4096},
        }
        dump(self.config_path, self.config)
        # The provider configuration will receive the runtime freeze digest. It
        # cannot itself be a prepared input to that same freeze.
        prepared_paths = [PACKET / 'tools/freeze.py', PACKET / 'tools/lab.pl',
                          Path('tools/agent_eval/executor.py'), PACKET / 'METER_PROBE.txt',
                          PACKET / 'documents.json', PACKET / 'public/cases.json', PACKET / 'judge/cases.json']
        dump(self.base / 'prepared-manifest.json', {
            'synthetic_test_fixture': True,
            'files': {str(p): sha((self.root / p).read_bytes()) for p in prepared_paths},
        })
        self.command(['git', '-c', 'init.defaultBranch=main', 'init', '--quiet'])
        self.command(['git', '-c', 'user.name=Synthetic Tests', '-c', 'user.email=synthetic@example.invalid',
                      '-c', 'commit.gpgsign=false', 'commit', '--quiet', '--allow-empty', '-m', 'synthetic fixture'])

    def command(self, args):
        return subprocess.run(args, cwd=self.root, env=self.environment, capture_output=True, timeout=15, check=True)

    def freeze(self, *extra):
        return subprocess.run([sys.executable, str(self.base / 'tools/freeze.py'), *extra],
                              cwd=self.root, env=self.environment, capture_output=True, timeout=15)

    def driver(self):
        spec = importlib.util.spec_from_file_location('synthetic_freeze_executor', self.executor_path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module.Config(self.config_path)

    def close(self):
        self.temporary.cleanup()


class FreezeIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(shutil.which('perl'), 'Perl is required for the neutral lab probe')
        self.assertIsNotNone(shutil.which('git'), 'Git is required for the synthetic revision')
        self.fixture = SyntheticFreeze()
        self.addCleanup(self.fixture.close)

    def assert_no_runtime_artifacts(self, fixture):
        self.assertFalse(fixture.frozen_path.exists())
        self.assertFalse(fixture.compiler_config_path.exists())
        self.assertFalse(fixture.config_path.with_suffix('.json.pending').exists())
        self.assertFalse((fixture.base / 'runs').exists())

    def test_freeze_binding_is_acyclic_and_driver_and_neutral_probe_agree(self):
        f = self.fixture
        before = f.driver().binding()
        result = f.freeze()
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        summary = json.loads(result.stdout)
        self.assertEqual(summary['status'], 'frozen_not_qualified')
        self.assertEqual(summary['participant_sessions'], 0)
        frozen = json.loads(f.frozen_path.read_bytes())
        config = json.loads(f.config_path.read_bytes())
        self.assertEqual(config['freeze']['sha256'], sha(f.frozen_path.read_bytes()))
        after = f.driver()
        after.assert_current()
        self.assertEqual(after.blockers(), [])
        for key in ('experiment_id', 'model_settings_sha256', 'executor_sha256'):
            self.assertEqual(frozen[key], after.binding()[key])
            self.assertEqual(before[key], after.binding()[key])
        self.assertEqual(frozen['identity_files'][str(f.compiler_config_path)], sha(f.compiler_config_path.read_bytes()))
        self.assertNotIn(str(f.config_path), frozen['identity_files'])
        self.assertNotIn(str(f.frozen_path), frozen['identity_files'])
        self.assertEqual(frozen['toolchain_files'][str(f.zig)], sha(f.zig.read_bytes()))
        probe = f.command(['perl', str(f.base / 'tools/lab.pl'), 'probe', 'doc', 'meter-probe', '1'])
        page = json.loads(probe.stdout)
        self.assertEqual(page['content'].encode('utf-8'), f.neutral)
        self.assertEqual(page['bytes'], len(f.neutral))
        self.assertEqual(page['sha256'], sha(f.neutral))
        self.assertFalse((f.base / 'runs').exists())
        self.assertFalse((f.base / 'evidence/launch-qualification.json').exists())

    def test_invalid_static_configuration_writes_no_runtime_artifacts(self):
        cases = [
            ('missing_model', lambda c: c['provider'].__setitem__('requested_model', None)),
            ('provider_limit', lambda c: c['limits'].__setitem__('max_provider_requests', 0)),
            ('lab_timeout', lambda c: c['lab'].__setitem__('timeout_seconds', 0)),
            ('lab_output', lambda c: c['lab'].__setitem__('output_bytes', 0)),
            ('wrong_lab', lambda c: c['lab'].__setitem__('script', 'other.pl')),
        ]
        for name, mutate in cases:
            with self.subTest(name=name):
                bad = copy.deepcopy(self.fixture.config)
                mutate(bad)
                dump(self.fixture.config_path, bad)
                original = self.fixture.config_path.read_bytes()
                result = self.fixture.freeze()
                self.assertNotEqual(result.returncode, 0, name)
                self.assert_no_runtime_artifacts(self.fixture)
                self.assertEqual(self.fixture.config_path.read_bytes(), original)

    def test_wrong_zig_override_fails_before_writing(self):
        f = self.fixture
        other = f.home / 'toolchains/other/zig'
        put(other, f.zig.read_bytes())
        other.chmod(0o700)
        original = f.config_path.read_bytes()
        result = f.freeze('--zig', str(other))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--zig must match', result.stderr.decode())
        self.assert_no_runtime_artifacts(f)
        self.assertEqual(f.config_path.read_bytes(), original)

    def test_second_freeze_refuses_to_overwrite(self):
        f = self.fixture
        first = f.freeze()
        self.assertEqual(first.returncode, 0, first.stderr.decode())
        saved = {p: p.read_bytes() for p in (f.config_path, f.frozen_path, f.compiler_config_path)}
        second = f.freeze()
        self.assertNotEqual(second.returncode, 0)
        self.assertIn('do not overwrite an experiment', second.stderr.decode())
        for path, original in saved.items():
            self.assertEqual(path.read_bytes(), original)
        self.assertFalse((f.base / 'runs').exists())


if __name__ == '__main__':
    unittest.main()
