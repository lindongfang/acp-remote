"""HN-002. Run in the checkout root with Python 3."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

src = Path("clients/web/src")
files = sorted(src.rglob("*.ts"))
edges = {}
failures = []


def check(ok, problem):
    if not ok:
        failures.append(problem)


def layer(path):
    part = path.relative_to(src).parts[0]
    return {"protocol": 0, "views": 0, "pairing": 1, "management": 1,
            "workbench": 2, "main.ts": 3}.get(part)


for file in files:
    content = file.read_text(encoding="utf8")
    rel = file.relative_to(src).as_posix()
    limit = {"main.ts": 250, "management/session-protocol.ts": 120}.get(rel)
    if rel.startswith(("workbench/", "management/session-protocol/")) or rel == "management/history-page-view.ts":
        limit = 650
    if limit:
        lines = len(content.splitlines())
        print(f"{rel} {lines}/{limit}")
        check(lines <= limit, f"line budget {rel}")
    check(not re.search(r"\b(innerHTML|outerHTML|insertAdjacentHTML)\b", content), f"HTML sink {rel}")
    edges[file] = []
    for spec in re.findall(r"""(?:\bfrom\s*|\bimport\s*\(?\s*)["'](\.[^"']+)["']""", content):
        target = file.parent.joinpath(spec).resolve().relative_to(Path.cwd())
        target = next((p for p in (Path(str(target) + ".ts"), target / "index.ts") if p in files), None)
        if target is not None:
            edges[file].append(target)
            if layer(file) is not None and layer(target) is not None:
                check(layer(target) <= layer(file), f"reverse dependency {rel} -> {target}")


def visit(file, path):
    if file in path:
        failures.append(f"cycle {path[path.index(file):]} -> {file}")
    elif file not in seen:
        seen.add(file)
        for dependency in edges[file]:
            visit(dependency, path + [file])


seen = set()
for file in files:
    visit(file, [])
check(not json.loads(Path("clients/web/package.json").read_text()).get("dependencies"), "runtime dependencies")
diff = subprocess.check_output(["git", "diff", "--unified=0", "4dd96797", "--",
                                str(src / "management/session-protocol.ts")], text=True)
for line in diff.splitlines():
    if line.startswith(("+", "-")) and not line.startswith(("+++", "---")):
        check(line.startswith("+export "), f"barrel {line}")
manifest = Path("clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md")
for row in manifest.read_text().splitlines():
    size, _, digest, name = row.split()
    data = Path(name).read_bytes()
    check(len(data) == int(size) and hashlib.sha256(data).hexdigest() == digest, name)
print(f"{len(files)} source modules, {sum(map(len, edges.values()))} import edges")
for failure in failures:
    print(failure)
raise SystemExit(bool(failures))
