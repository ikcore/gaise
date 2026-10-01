"""Bump the GAISe workspace version everywhere it is written down.

Usage:
    python .claude/skills/model-release/scripts/bump_version.py minor   # 4.1.0 -> 4.2.0
    python .claude/skills/model-release/scripts/bump_version.py major   # 4.1.0 -> 5.0.0
    python .claude/skills/model-release/scripts/bump_version.py patch   # 4.1.0 -> 4.1.1
    python .claude/skills/model-release/scripts/bump_version.py 4.3.0   # explicit

Updates `[workspace.package].version`, every internal `gaise*` requirement
under `[workspace.dependencies]` (a stale one breaks `cargo publish`, see
wiki/releasing.md), and the dependency snippets in tracked Markdown files
(CHANGELOG.md excluded). Prints `old new` on success. Cargo.lock is refreshed
by the next cargo build.
"""
import io
import re
import subprocess
import sys

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
).stdout.strip() + "/"


def read(path):
    with io.open(ROOT + path, newline="", encoding="utf-8") as f:
        return f.read()


def write(path, text):
    with io.open(ROOT + path, "w", newline="", encoding="utf-8") as f:
        f.write(text)


cargo = read("Cargo.toml")
match = re.search(r'\[workspace\.package\][^\[]*?\nversion = "(\d+)\.(\d+)\.(\d+)"', cargo)
if not match:
    sys.exit("workspace version not found")
major, minor, patch = map(int, match.groups())
old = f"{major}.{minor}.{patch}"
arg = sys.argv[1] if len(sys.argv) > 1 else ""
if arg == "major":
    new = f"{major + 1}.0.0"
elif arg == "minor":
    new = f"{major}.{minor + 1}.0"
elif arg == "patch":
    new = f"{major}.{minor}.{patch + 1}"
elif re.fullmatch(r"\d+\.\d+\.\d+", arg):
    new = arg
else:
    sys.exit(__doc__)

start, end = match.span()
cargo = cargo[:start] + match.group(0).replace(f'"{old}"', f'"{new}"') + cargo[end:]
cargo, count = re.subn(
    rf'^(gaise[\w-]* = \{{[^\n]*version = "){re.escape(old)}(")',
    rf"\g<1>{new}\g<2>",
    cargo,
    flags=re.M,
)
if count == 0:
    sys.exit("no internal dependency requirements found")
write("Cargo.toml", cargo)

docs = subprocess.run(
    ["git", "grep", "-l", "-F", old, "--", "*.md"], cwd=ROOT, capture_output=True, text=True
).stdout.split()
for path in docs:
    if path == "CHANGELOG.md":
        continue
    text = read(path)
    write(path, text.replace(f'version = "{old}"', f'version = "{new}"')
          .replace(f"release {old}", f"release {new}")
          .replace(f"`{old}`", f"`{new}`"))
print(old, new)
