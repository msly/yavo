#!/usr/bin/env python3
"""Small controls for tier escalation and changed-case selection; no builds."""
import copy
import importlib.util
import json
from pathlib import Path, PureWindowsPath
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


plan = load("validation_plan_control", "tools/ci/validation_plan.py")
core = load("core_cases_control", "selfhost/core_cases.py")


class CoreSelectionTests(unittest.TestCase):
    def setUp(self):
        self.prior = [{"id": name, "kind": "run", "file": "run/" + name + ".yv"}
                      for name in core.CORE_IDS]
        self.prior += [
            {"id": "control_changed", "kind": "run", "file": "run/control_changed.yv"},
            {"id": "control_unrelated", "kind": "run", "file": "run/control_unrelated.yv"},
            {"id": "control_module_a", "kind": "run", "file": "modules/alpha/main.yv"},
            {"id": "control_module_b", "kind": "run", "file": "modules/beta/main.yv"},
        ]

    def select(self, current, changed=(), windows=False):
        def git(argv, **kwargs):
            if argv[:2] == ["git", "show"]:
                return json.dumps({"cases": self.prior}).encode()
            if argv[:3] == ["git", "diff", "--name-only"]:
                return "\n".join(changed)
            self.fail("unexpected Git command: " + repr(argv))
        with patch.object(core.subprocess, "check_output", side_effect=git):
            # ROOT was established at import. This makes later platform-dependent
            # Path use behave like Windows while Git still emits POSIX paths.
            with patch.object(core, "Path", PureWindowsPath if windows else Path):
                selected, receipt = core.select(current, "test-baseline")
        return {row["id"] for row in selected}, receipt

    def test_one_run_change_does_not_select_its_whole_directory(self):
        selected, receipt = self.select(self.prior, ["selfhost/tests/run/control_changed.yv"])
        self.assertEqual(selected, set(core.CORE_IDS) | {"control_changed"})
        self.assertEqual(receipt["reasons"]["control_changed"], ["changed fixture"])

    def test_module_helper_uses_git_posix_paths_on_windows(self):
        changed = ["selfhost/tests/modules/alpha/helper.yv"]
        for windows in (False, True):
            with self.subTest(windows=windows):
                selected, receipt = self.select(self.prior, changed, windows=windows)
                self.assertEqual(selected, set(core.CORE_IDS) | {"control_module_a"})
                self.assertIn("changed sibling module or fixture", receipt["reasons"]["control_module_a"])

    def test_new_or_changed_manifest_entries_are_always_selected(self):
        current = copy.deepcopy(self.prior)
        current.append({"id": "control_new", "kind": "run", "file": "run/new.yv"})
        next(row for row in current if row["id"] == "control_unrelated")["stdout"] = "changed expectation\n"
        selected, receipt = self.select(current)
        self.assertEqual(selected, set(core.CORE_IDS) | {"control_new", "control_unrelated"})
        self.assertEqual(receipt["reasons"]["control_new"], ["new or changed expectation"])

    def test_changed_prefix_or_example_selects_dependent_case(self):
        row = next(row for row in self.prior if row["id"] == "control_unrelated")
        row.update(source_prefix="model.yv", example="owned_token_store.yv")
        for changed in ("selfhost/model.yv", "selfhost/examples/owned_token_store.yv"):
            with self.subTest(changed=changed):
                selected, _ = self.select(self.prior, [changed])
                self.assertEqual(selected, set(core.CORE_IDS) | {"control_unrelated"})

    def test_missing_semantic_spine_case_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "core cases disappeared"):
            self.select(self.prior[1:])


class TierTests(unittest.TestCase):
    def policy(self, **changes):
        return {"schema": "yavo-validation-policy/1", "batch": 52, "full_every_batches": 5,
                "milestone": False, "baseline_commit": "test-baseline", **changes}

    def test_cadence_milestone_and_release_cannot_be_downgraded(self):
        scenarios = [(self.policy(batch=55), "", "batch cadence"),
                     (self.policy(milestone=True), "", "declared milestone"),
                     (self.policy(), "refs/tags/v0.1.0", "release tag")]
        for policy, ref, reason in scenarios:
            for requested in ("auto", "core", "full"):
                with self.subTest(policy=policy, ref=ref, requested=requested):
                    result = plan.resolve(policy, requested, ref)
                    self.assertEqual(result["tier"], "full")
                    self.assertIn(reason, result["reasons"])

    def test_ordinary_batch_is_core_and_manual_full_escalates(self):
        for requested in ("auto", "core"):
            self.assertEqual(plan.resolve(self.policy(), requested, "refs/heads/main")["tier"], "core")
        self.assertEqual(plan.resolve(self.policy(), "full")["tier"], "full")

    def test_qualifications_are_explicit_single_batch_without_full_escalation(self):
        options = {"p0_sanitizer_qualification_batch": 54, "performance_qualification_batch": 54}
        for batch, tier, qualified in ((54, "core", True), (55, "full", False), (56, "core", False)):
            result = plan.resolve(self.policy(batch=batch, **options), "core", "refs/heads/main")
            self.assertEqual(result["tier"], tier)
            self.assertEqual(result["p0_sanitizer_qualification"], qualified)
            self.assertEqual(result["paired_performance"], qualified)
        ordinary = plan.resolve(self.policy(batch=54))
        self.assertFalse(ordinary["p0_sanitizer_qualification"])
        self.assertFalse(ordinary["paired_performance"])

    def test_invalid_cadence_and_boolean_policy_are_rejected(self):
        for changes in ({"batch": True}, {"full_every_batches": 0}, {"milestone": "false"},
                        {"p0_sanitizer_qualification_batch": True}, {"performance_qualification_batch": 0}):
            with self.assertRaises(ValueError):
                plan.resolve(self.policy(**changes))


if __name__ == "__main__":
    unittest.main(verbosity=2)
