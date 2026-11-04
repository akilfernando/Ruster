"""Policy regressions; fixtures use Git but never compile Rust."""

import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("ruster_check", Path(__file__).resolve().parents[1] / "check.py")
check = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = check
SPEC.loader.exec_module(check)


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.git("init", "-q")
        self.git("config", "user.email", "runner@example.invalid")
        self.git("config", "user.name", "Runner Test")
        self.write("README.md", "# Fixture\n")
        self.write("crates/ruster-core/src/lib.rs", "// Core\n")
        self.write("crates/ruster-cli/src/main.rs", "// CLI\n")
        self.commit()
        self.workspace = {
            "workspace_members": ["core", "cli"],
            "packages": [
                {"id": "core", "name": "ruster-core", "manifest_path": str(self.root / "crates/ruster-core/Cargo.toml"), "dependencies": []},
                {"id": "cli", "name": "ruster-cli", "manifest_path": str(self.root / "crates/ruster-cli/Cargo.toml"), "dependencies": [
                    {"name": "renamed-core", "path": str(self.root / "crates/ruster-core")},
                ]},
            ],
        }

    def git(self, *args):
        return check.git(self.root, *args)

    def write(self, path, content):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def plan(self, base=None):
        paths, reason = check.changed_files(self.root, base)
        return check.choose_plan(self.root, paths, reason, self.workspace)

    def test_docs_only_never_needs_cargo(self):
        self.write("README.md", "# Updated\n")
        paths, reason = check.changed_files(self.root)
        with patch.object(check, "metadata", side_effect=AssertionError("Cargo was invoked")):
            plan = check.choose_plan(self.root, paths, reason)
        self.assertEqual(plan.scope, "docs")
        self.assertEqual(check.commands(plan), [])

    def test_cli_only_selects_cli(self):
        self.write("crates/ruster-cli/src/main.rs", "// Updated\n")
        self.assertEqual(self.plan().packages, ("ruster-cli",))

    def test_core_selects_dependents_with_alias(self):
        self.write("crates/ruster-core/src/lib.rs", "// Updated\n")
        self.assertEqual(self.plan().packages, ("ruster-cli", "ruster-core"))

    def test_unknown_build_and_runner_inputs_are_full(self):
        for path in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates/ruster-cli/build.rs", "scripts/check.py", "assets/example.bin"):
            with self.subTest(path=path):
                self.assertEqual(check.choose_plan(self.root, {path}, workspace=self.workspace).scope, "full")

    def test_unborn_is_full_but_explicit_invalid_base_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            check.git(root, "init", "-q")
            paths, reason = check.changed_files(root)
            self.assertEqual(check.choose_plan(root, paths, reason).scope, "full")
            for base in ("HEAD", "missing-ref"):
                with self.subTest(base=base), self.assertRaises(subprocess.CalledProcessError):
                    check.changed_files(root, base)

    def test_invalid_base_fails(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.plan("missing-ref")

    def test_union_of_committed_staged_unstaged_untracked(self):
        base = self.git("rev-parse", "HEAD").decode().strip()
        self.write("docs/committed.md", "# Committed\n")
        self.commit()
        self.write("docs/staged.md", "# Staged\n")
        self.git("add", "docs/staged.md")
        self.write("README.md", "# Unstaged\n")
        self.write("docs/untracked.md", "# Untracked\n")
        paths, reason = check.changed_files(self.root, base)
        self.assertIsNone(reason)
        self.assertEqual(paths, {"docs/committed.md", "docs/staged.md", "README.md", "docs/untracked.md"})

    def test_staged_change_canceled_by_worktree_is_still_checked(self):
        self.write("crates/ruster-cli/src/main.rs", "// Staged\n")
        self.git("add", "crates/ruster-cli/src/main.rs")
        self.write("crates/ruster-cli/src/main.rs", "// CLI\n")
        self.assertEqual(self.plan().packages, ("ruster-cli",))

    def test_markdown_inventory_ignores_build_output_and_catches_deleted_target(self):
        self.write(".gitignore", "target/\n")
        self.write("docs/target.md", "# Target\n")
        self.write("docs/source.md", "[Target](target.md)\n")
        self.commit()
        self.write("target/generated.md", "[broken](missing.md)\n")
        (self.root / "docs/target.md").unlink()
        paths = check.markdown_files(self.root)
        self.assertNotIn("target/generated.md", paths)
        self.assertIn("docs/source.md", paths)
        self.assertEqual(self.plan().scope, "docs")
        with self.assertRaisesRegex(RuntimeError, "target.md"):
            check.validate_markdown(self.root, paths)

    def test_deleted_rust_and_cross_package_rename_keep_both_owners(self):
        self.git("mv", "crates/ruster-core/src/lib.rs", "crates/ruster-cli/src/moved.rs")
        paths, _ = check.changed_files(self.root)
        self.assertIn("crates/ruster-core/src/lib.rs", paths)
        self.assertIn("crates/ruster-cli/src/moved.rs", paths)
        self.assertEqual(self.plan().packages, ("ruster-cli", "ruster-core"))

    def test_clean_tree_is_noop(self):
        self.assertEqual(self.plan().scope, "none")

    def test_markdown_links_and_whitespace(self):
        self.write("docs/valid.md", "[home](../README.md#fixture)\n[web](https://example.invalid)\n[reference]: ../README.md\n")
        check.validate_markdown(self.root, {"docs/valid.md", "deleted.md"})
        for text in ("[broken](missing.md)\n", "trailing \n", "no newline", "[ref]: missing.md\n"):
            with self.subTest(text=text):
                self.write("docs/broken.md", text)
                with self.assertRaises(RuntimeError):
                    check.validate_markdown(self.root, {"docs/broken.md"})

    def test_commands_include_doc_tests_and_fail_fast(self):
        plan = check.Plan("packages", "fixture", ("ruster-cli",))
        commands = check.commands(plan)
        self.assertEqual([command[1] for command in commands], ["fmt", "clippy", "test"])
        self.assertIn("--locked", commands[-1])
        self.assertNotIn("--all-targets", commands[-1])
        self.assertNotIn("--no-default-features", commands[-1])
        self.assertEqual(check.commands(check.Plan("full", "fixture"))[0][1:4], ["-m", "unittest", "discover"])
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()), patch.object(check, "__file__", str(self.root / "scripts/check.py")), patch.object(check, "changed_files", return_value=({"unknown"}, None)), patch.object(check, "validate_markdown"), patch.object(check, "markdown_files", return_value=set()), patch.object(check, "cargo_binary", return_value="cargo"), patch.object(check.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")) as run:
            self.assertEqual(check.main([]), 1)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(run.call_args.args[0][1:4], ["-m", "unittest", "discover"])


if __name__ == "__main__":
    unittest.main()
