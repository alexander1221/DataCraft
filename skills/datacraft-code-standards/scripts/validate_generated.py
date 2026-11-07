"""Static artifact checks for DataCraft (Python 3.11+, standard library).

User's Wilkinson-style convention; adapted from code-generation-standards v4.

§1 Problem: detect malformed serialized artifacts and nonportable source names
without executing project code. This is not a Rust/Scala compiler or audit.
§2 Algorithm: walk a finite tree without reparse points; prune build caches;
check relative names, then parse supported text formats. Accumulate failures.
§3 Applications: preflight generated source trees and review script encodings.
§4 Specification: root is an existing directory; max_rel_path is a positive
UTF-16 code-unit budget (default 200). Input files are never modified. Exit 0
means selected static checks passed, 1 means a finding/error, 2 is CLI misuse.
§5 Procedures: inspect_tree, component_error, main; implementation below.
§6 Notes: Python AST does not import modules. JSON/TOML parsers read one file
at a time. Build caches are excluded. No network, shell, compiler or JVM runs.
§7 Test cases: --selftest covers malformed JSON/TOML/Python, ASCII shell
boundaries, case/NFC collisions, UTF-16 lengths, and read-only traversal.
Actual execution results belong in docs/STANDARDS_ADAPTATION.md.
§8' Robustness: report filesystem/encoding/parser errors; do not follow links.
YAML, Rust, Scala, PowerShell syntax and external facts remain NOT_RUN. Runtime
resource use is proportional to tree entries plus the largest parsed file.
"""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass, field
import io
import json
from pathlib import Path
import re
import stat
import sys
import tempfile
import tomllib
import unicodedata
import unittest


EXCLUDED = {
    ".git", ".tools", "target", ".venv", "venv", "node_modules",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache",
    ".bsp", ".metals",
}
RESERVED = re.compile(r"^(CON|PRN|AUX|NUL|COM[1-9¹²³]|LPT[1-9¹²³])$", re.I)


@dataclass
class Findings:
    errors: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)
    files: int = 0
    parsed: int = 0


def utf16_length(value: str) -> int:
    return len(value.encode("utf-16-le", errors="surrogatepass")) // 2


def component_error(name: str) -> str | None:
    if name.endswith((" ", ".")):
        return "trailing dot or space"
    if any(ord(c) < 32 or c in '<>:"/\\|?*' for c in name):
        return "invalid Windows filename character"
    if RESERVED.fullmatch(name.split(".", 1)[0]):
        return "reserved Windows filename"
    return None


def inspect_tree(root: Path, max_rel_path: int = 200) -> Findings:
    if max_rel_path < 1:
        raise ValueError("max_rel_path must be positive")
    root = root.resolve(strict=True)
    if not root.is_dir():
        raise ValueError("root must be a directory")
    result = Findings()
    seen: dict[str, str] = {}
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            entries = sorted(directory.iterdir(), key=lambda p: p.name)
        except OSError as error:
            result.errors.append(f"{directory}: {error}")
            continue
        for path in entries:
            rel = path.relative_to(root).as_posix()
            try:
                metadata = path.lstat()
                # Windows junctions/reparse points can also lead outside the tree.
                reparse = getattr(metadata, "st_file_attributes", 0) & 0x400
                if path.is_symlink() or reparse:
                    result.skipped.append(f"link/reparse point: {rel}")
                    continue
                if stat.S_ISDIR(metadata.st_mode) and path.name in EXCLUDED:
                    continue
                reason = component_error(path.name)
                if reason:
                    result.errors.append(f"{rel}: {reason}")
                if utf16_length(rel) > max_rel_path:
                    result.errors.append(f"{rel}: relative path exceeds {max_rel_path} UTF-16 units")
                normalized = unicodedata.normalize("NFC", rel)
                if rel != normalized:
                    result.errors.append(f"{rel}: filename is not NFC normalized")
                key = normalized.casefold()
                if key in seen:
                    result.errors.append(f"{rel}: case/NFC collision with {seen[key]}")
                seen[key] = rel
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(path)
                    continue
                if not stat.S_ISREG(metadata.st_mode):
                    result.skipped.append(f"non-regular file: {rel}")
                    continue
                result.files += 1
                suffix = path.suffix.lower()
                if suffix in {".ps1", ".bat", ".cmd"}:
                    path.read_bytes().decode("ascii")
                    result.parsed += 1  # Encoding check only; not a shell parser.
                elif suffix in {".json", ".dcraft", ".toml", ".py"}:
                    text = path.read_text(encoding="utf-8")
                    if suffix in {".json", ".dcraft"}:
                        def reject_constant(value: str) -> None:
                            raise ValueError(f"nonstandard JSON number: {value}")
                        json.loads(text, parse_constant=reject_constant)
                    elif suffix == ".toml":
                        tomllib.loads(text)
                    else:
                        ast.parse(text, filename=rel)
                    result.parsed += 1
            except (OSError, UnicodeError, SyntaxError, ValueError) as error:
                result.errors.append(f"{rel}: {type(error).__name__}: {error}")
    return result


def emit(message: str) -> None:
    # Diagnostics remain readable through Windows legacy code-page pipelines.
    print(message.encode("ascii", errors="backslashreplace").decode("ascii"))


def selftest() -> int:
    class Checks(unittest.TestCase):
        def check_file(self, name: str, data: bytes) -> Findings:
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / name
                path.write_bytes(data)
                result = inspect_tree(Path(directory))
                self.assertEqual(path.read_bytes(), data)
                return result

        def test_valid_artifacts(self):
            for name, value in [("project.dcraft", b'{"format_version":1}'),
                                ("Cargo.toml", b'[workspace]\nmembers = []\n'),
                                ("helper.py", b'x = 1\n')]:
                result = self.check_file(name, value)
                self.assertEqual(result.errors, [])
                self.assertEqual(result.parsed, 1)

        def test_malformed_formats(self):
            for name, value in [("bad.json", b'{'), ("bad.toml", b'[broken'),
                                ("bad.py", b'def broken(:'), ("bad.dcraft", b'{"x":NaN}')]:
                self.assertTrue(self.check_file(name, value).errors)

        def test_script_encoding(self):
            self.assertEqual(self.check_file("setup.ps1", b'Write-Output "OK"').errors, [])
            self.assertTrue(self.check_file("setup.ps1", 'Write-Output "\u0434\u0430"'.encode()).errors)

        def test_reserved_names(self):
            for name in ["NUL.txt", "COM1.log", "COM\u00b9.txt", "LPT9", "bad.", "a:"]:
                self.assertIsNotNone(component_error(name))
            self.assertIsNone(component_error("com10.csv"))

        def test_utf16_budget(self):
            self.assertEqual(utf16_length("\U0001f600"), 2)
            with tempfile.TemporaryDirectory() as directory:
                (Path(directory) / "long.json").write_text("{}", encoding="utf-8")
                self.assertTrue(inspect_tree(Path(directory), 4).errors)

        def test_excludes_build_cache(self):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "target").mkdir()
                (root / "target" / "bad.json").write_text("{", encoding="utf-8")
                self.assertEqual(inspect_tree(root).files, 0)

        def test_nfc_collision(self):
            # NFC/NFD are distinct filenames on Windows and Linux. If the host
            # normalizes them, report a skip rather than claim this case passed.
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for name in ["\u00e9.json", "e\u0301.json"]:
                    (root / name).write_text("{}", encoding="utf-8")
                if len(list(root.iterdir())) != 2:
                    self.skipTest("host filesystem normalizes names")
                self.assertTrue(any("collision" in e for e in inspect_tree(root).errors))

        def test_case_collision(self):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for name in ["A.json", "a.json"]:
                    (root / name).write_text("{}", encoding="utf-8")
                if len(list(root.iterdir())) != 2:
                    self.skipTest("host filesystem is case-insensitive")
                self.assertTrue(any("collision" in e for e in inspect_tree(root).errors))

    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream, verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(Checks))
    emit(stream.getvalue())
    return 0 if result.wasSuccessful() else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", nargs="?", type=Path)
    parser.add_argument("--max-rel-path", type=int, default=200)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv)
    if args.selftest:
        return selftest()
    if args.root is None or args.max_rel_path < 1:
        parser.error("provide a root directory and a positive path budget")
    try:
        result = inspect_tree(args.root, args.max_rel_path)
    except (OSError, ValueError) as error:
        emit(f"[FAIL] {error}")
        return 1
    for error in result.errors:
        emit(f"[FAIL] {error}")
    for skipped in result.skipped:
        emit(f"[NOT_RUN] {skipped}")
    emit(f"[SUMMARY] files={result.files} parsed_or_encoding_checked={result.parsed} errors={len(result.errors)}")
    emit("[NOT_RUN] Rust/Scala compilation, YAML and shell syntax, dependency/network verification, runtime tests")
    return 1 if result.errors else 0


if __name__ == "__main__":
    sys.exit(main())
