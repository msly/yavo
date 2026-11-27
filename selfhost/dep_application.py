"""Dependency-index gate: two frontends, Debug/Release and exit cleanup."""
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _configuration():
    path = ROOT / "tools/ci/dep_support.py"
    spec = importlib.util.spec_from_file_location("dep_application_configuration", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inputs():
    return _configuration().create_support().input_identities()


def check(selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe):
    return _configuration().create_bootstrap().check(
        selfhost, stage0, zig, directory, environment, frontend_environment, cc_flags, exe)
