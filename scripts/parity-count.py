#!/usr/bin/env python3
"""Recount the docs/parity/README.md index table from the entries' status lines.

Run after editing parity entries: `python3 scripts/parity-count.py` (rewrites the table rows,
the totals row and the percentage line in place; prints the totals).
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "parity"
STATUS = re.compile(r"^- \*\*Статус в ss13-rs:\*\* (✅|🟡|❌)", re.M)


def count(folder: pathlib.Path):
    files = sorted(folder.glob("*.md"))
    marks = [m for f in files for m in STATUS.findall(f.read_text(encoding="utf-8"))]
    return len(files), len(marks), marks.count("✅"), marks.count("🟡"), marks.count("❌")


readme = ROOT / "README.md"
text = readme.read_text(encoding="utf-8")
total = [0, 0, 0, 0, 0]


def row(m: re.Match) -> str:
    counts = count(ROOT / m.group(2))
    for i, c in enumerate(counts):
        total[i] += c
    return m.group(1) + " | ".join(str(c) for c in counts) + " |"


text = re.sub(r"^(\| [^|\n]+ \| \[[^\]]+\]\(([^)/]+)/\) \| ).*\|$", row, text, flags=re.M)
files, entries, done, partial, missing = total
text = re.sub(
    r"^\| \*\*Итого\*\* \|.*$",
    f"| **Итого** | | **{files}** | **{entries}** | **{done}** | **{partial}** | **{missing}** |",
    text,
    flags=re.M,
)
pct = lambda n: f"{100 * n / entries:.1f}%"
text = re.sub(
    r"Доля записей: .*$",
    f"Доля записей: ✅ {pct(done)}, 🟡 {pct(partial)}, ❌ {pct(missing)}.",
    text,
    flags=re.M,
)
readme.write_text(text, encoding="utf-8")
print(f"entries {entries}: ✅ {done}, 🟡 {partial}, ❌ {missing}")


# The readiness page (docs/parity/PROGRESS.html) from the same entries.
import runpy  # noqa: E402

runpy.run_path(str(pathlib.Path(__file__).with_name("parity_progress.py")), run_name="__main__")
