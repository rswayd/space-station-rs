#!/usr/bin/env python3
"""Writes crates/ss13_game/src/science/cell_line_data.rs and the vat results the converter must
emit (crates/ss13_mapload/src/cell_line_types.rs): tg's cell lines
(`code/modules/research/xenobiology/vatgrowing/samples/cell_lines/*.dm`), their weighted tables
(`GLOBAL_LIST_INIT_TYPED(cell_line_tables, ...)` in `code/_globalvars/lists/xenobiology.dm`) and
which atom types are swabable (`AddElement(/datum/element/swabable, ...)` sites, keyed by the type
whose proc adds it), from the BYOND checkout's DM source.

    python3 -I scripts/gen-cell-lines.py [<BandaStation checkout>]

Rerun after BandaStation changes a cell line, a table or a swabable site.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates/ss13_game/src/science/cell_line_data.rs")
TYPES_OUT = os.path.join(ROOT, "crates/ss13_mapload/src/cell_line_types.rs")
SOURCE = sys.argv[1] if len(sys.argv) > 1 else "../BandaStation"

ROOT_PATH = "/datum/micro_organism/cell_line"
DEFINES = {"VAT_GROWTH_RATE": "4", "REAGENTS_METABOLISM": "0.2"}
DEFAULTS = {
    "desc": "White fluid that tastes like salty coins and milk",
    "required_reagents": [],
    "supplementary_reagents": [],
    "suppressive_reagents": [],
    "virus_suspectibility": 1.0,
    "growth_rate": 4.0,
    "consumption_rate": 0.2,
    "resulting_atom": "",
    "resulting_atom_count": 1,
}


def read(path):
    with open(os.path.join(SOURCE, path), encoding="utf-8", errors="replace") as f:
        return f.read()


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"(?<!:)//[^\n]*", "", text)


def blocks(text):
    """(header, body) of every column-0 definition."""
    out = []
    header, body = None, []
    for line in text.split("\n"):
        if line and not line[0].isspace():
            if header is not None:
                out.append((header, "\n".join(body)))
            header, body = line.strip(), []
        elif header is not None:
            body.append(line)
    if header is not None:
        out.append((header, "\n".join(body)))
    return out


def balanced(text, start):
    """The text inside the parenthesis opening at `start`."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1 : i]
    raise ValueError("unbalanced")


def num(expr, parent):
    expr = expr.strip()
    expr = re.sub(r"parent_type::(\w+)", lambda m: repr(parent[m.group(1)]), expr)
    for k, v in DEFINES.items():
        expr = re.sub(r"\b%s\b" % k, v, expr)
    if not re.fullmatch(r"[0-9.+\-*/ ()]+", expr):
        raise ValueError("not a number: " + expr)
    return float(eval(expr, {"__builtins__": {}}))  # noqa: S307 - arithmetic only


def parse_list(inner):
    out = []
    for entry in inner.split(","):
        entry = entry.strip()
        if not entry:
            continue
        if "=" in entry:
            path, value = entry.split("=", 1)
            out.append((path.strip(), float(value.strip())))
        else:
            out.append((entry, None))
    return out


def cell_lines():
    lines = {}
    order = []
    for name in ("common.dm", "organs.dm"):
        text = strip_comments(
            read("code/modules/research/xenobiology/vatgrowing/samples/cell_lines/" + name)
        )
        for header, body in blocks(text):
            if not header.startswith(ROOT_PATH) or "(" in header:
                continue
            path = header
            parent_path = path.rsplit("/", 1)[0]
            parent = lines.get(parent_path, DEFAULTS)
            line = dict(parent)
            for var in ("required_reagents", "supplementary_reagents", "suppressive_reagents"):
                m = re.search(r"\b%s\s*=\s*list\(" % var, body)
                if m:
                    line[var] = parse_list(balanced(body, m.end() - 1))
            m = re.search(r'\bdesc\s*=\s*"([^"]*)"', body)
            if m:
                line["desc"] = m.group(1)
            m = re.search(r"\bresulting_atom\s*=\s*(/[\w/]+)", body)
            if m:
                line["resulting_atom"] = m.group(1)
            for var in (
                "virus_suspectibility",
                "growth_rate",
                "consumption_rate",
                "resulting_atom_count",
            ):
                m = re.search(r"^\s*%s\s*=\s*([^\n]+)$" % var, body, flags=re.M)
                if m:
                    line[var] = num(m.group(1), parent)
            lines[path] = line
            if path not in order:
                order.append(path)
    return lines, order


def tables():
    text = strip_comments(read("code/_globalvars/lists/xenobiology.dm"))
    start = text.index("cell_line_tables")
    inner = balanced(text, text.index("list(", start) + 4)
    out = []
    for m in re.finditer(r"(CELL_LINE_\w+)\s*=\s*list\(", inner):
        entries = parse_list(balanced(inner, m.end() - 1))
        out.append((m.group(1), [(p, int(w or 1)) for p, w in entries]))
    return out


def dm_files():
    for base in ("code", "modular_bandastation"):
        for dirpath, _, files in os.walk(os.path.join(SOURCE, base)):
            for f in files:
                if f.endswith(".dm"):
                    yield os.path.join(dirpath, f)


def swabables():
    out = {}
    for path in dm_files():
        with open(path, encoding="utf-8", errors="replace") as f:
            text = strip_comments(f.read())
        if "datum/element/swabable" not in text and "cell_line" not in text:
            continue
        headers = [
            (m.start(), m.group(1))
            for m in re.finditer(r"^(/[\w/]+)\(", text, flags=re.M)
        ]
        for m in re.finditer(r"AddElement\(\s*/datum/element/swabable\s*,", text):
            inner = balanced(text, m.start() + len("AddElement"))
            owners = [h for at, h in headers if at < m.start()]
            if not owners:
                continue
            args = [
                re.sub(r"^\w+\s*=\s*", "", a.strip())
                for a in re.split(r",(?![^(]*\))", inner)
            ][1:]
            args = [a for a in args if a]
            if not args or not args[0].startswith("CELL_LINE_"):
                continue
            amount = args[2] if len(args) > 2 else "1"
            chance = args[3] if len(args) > 3 else "10"
            r = re.fullmatch(r"rand\((\d+),\s*(\d+)\)", amount)
            if r:
                lo, hi = int(r.group(1)), int(r.group(2))
            elif amount.isdigit():
                lo = hi = int(amount)
            else:
                continue
            if not chance.isdigit():
                continue
            type_path = re.sub(r"/(proc|verb)$", "", owners[-1].rsplit("/", 1)[0])
            out.setdefault(type_path, (args[0], lo, hi, int(chance)))
        # `var/cell_line` types (organs, carps, carp meat) add the element in a parent proc.
        for header, body in blocks(text):
            if "(" in header or not header.startswith("/"):
                continue
            m = re.search(r"^\s*(?:var/)?cell_line\s*=\s*(CELL_LINE_\w+)", body, flags=re.M)
            if not m:
                continue
            if header.startswith("/obj/item/organ"):
                lo = re.search(r"^\s*cells_minimum\s*=\s*(\d+)", body, flags=re.M)
                hi = re.search(r"^\s*cells_maximum\s*=\s*(\d+)", body, flags=re.M)
                amount = (int(lo.group(1)) if lo else 1, int(hi.group(1)) if hi else 2)
                out.setdefault(header, (m.group(1), amount[0], amount[1], 10))
            elif header.startswith("/mob"):
                out.setdefault(header, (m.group(1), 1, 1, 5))
            else:
                out.setdefault(header, (m.group(1), 1, 1, 10))
    return sorted(out.items())


def rs(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def f32(v):
    s = repr(float(v))
    return s


def main():
    lines, order = cell_lines()
    out = [
        "// Generated by scripts/gen-cell-lines.py from the BYOND checkout's DM source. Do not edit.",
        "",
        "use super::cell_lines::{CellLine, Swabable};",
        "",
        "pub static CELL_LINES: &[CellLine] = &[",
    ]
    for path in order:
        line = lines[path]
        req = ", ".join(rs(p) for p, _ in line["required_reagents"])
        sup = ", ".join("(%s, %s)" % (rs(p), f32(v or 0)) for p, v in line["supplementary_reagents"])
        bad = ", ".join("(%s, %s)" % (rs(p), f32(v or 0)) for p, v in line["suppressive_reagents"])
        out.append("    CellLine {")
        out.append("        path: %s," % rs(path))
        out.append("        desc: %s," % rs(line["desc"]))
        out.append("        required: &[%s]," % req)
        out.append("        supplementary: &[%s]," % sup)
        out.append("        suppressive: &[%s]," % bad)
        out.append("        virus_susceptibility: %s," % f32(line["virus_suspectibility"]))
        out.append("        growth_rate: %s," % f32(line["growth_rate"]))
        out.append("        consumption_rate: %s," % f32(line["consumption_rate"]))
        out.append("        result: %s," % rs(line["resulting_atom"]))
        out.append("        result_count: %d," % int(line["resulting_atom_count"]))
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// `GLOB.cell_line_tables`: each table's cell lines with their weights.")
    out.append("pub static CELL_LINE_TABLES: &[(&str, &[(&str, u32)])] = &[")
    for key, entries in tables():
        body = ", ".join("(%s, %d)" % (rs(p), w) for p, w in entries)
        out.append("    (%s, &[%s])," % (rs(key), body))
    out.append("];")
    out.append("")
    out.append("/// The types that add `/datum/element/swabable` (most specific first by length).")
    out.append("pub static SWABABLES: &[Swabable] = &[")
    for path, (table, lo, hi, chance) in swabables():
        out.append(
            "    Swabable { path: %s, table: %s, amount: (%d, %d), virus_chance: %d },"
            % (rs(path), rs(table), lo, hi, chance)
        )
    out.append("];")
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")

    results = sorted({lines[p]["resulting_atom"] for p in order if lines[p]["resulting_atom"]})
    types = [
        "// Generated by scripts/gen-cell-lines.py from the BYOND checkout's DM source. Do not edit.",
        "",
        "/// What growing vats grow (`ss13_game::science::cell_lines`).",
        "pub const CELL_LINE_RESULTS: &[&str] = &[",
    ]
    types += ["    %s," % rs(p) for p in results]
    types.append("];")
    with open(TYPES_OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(types) + "\n")


if __name__ == "__main__":
    main()
