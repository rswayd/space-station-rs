#!/usr/bin/env python3
"""Writes crates/ss13_game/src/stack_recipes/data.rs and the result types the converter must emit
(crates/ss13_mapload/src/stack_recipe_types.rs): tg's stack crafting menus (`GLOBAL_LIST_INIT
(*_recipes, list(new /datum/stack_recipe(...), new /datum/stack_recipe_list(...), null, ...))`)
and which stack type shows which lists (`/obj/item/stack/.../get_main_recipes()`), from the
BYOND checkout's DM source. BandaStation's modpack edit of the wood list (`modular_bandastation/
objects/_objects.dm`: the plain wood tile recipe replaced by the wooden floor sublist) is applied.

    python3 -I scripts/gen-stack-recipes.py [<BandaStation checkout>]

Rerun after BandaStation changes a recipe list.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates/ss13_game/src/stack_recipes/data.rs")
TYPES_OUT = os.path.join(ROOT, "crates/ss13_mapload/src/stack_recipe_types.rs")
SOURCE = sys.argv[1] if len(sys.argv) > 1 else "../BandaStation"

CRAFT = {
    "CRAFT_MUST_BE_LEARNED": 1 << 0,
    "CRAFT_ONE_PER_TURF": 1 << 1,
    "CRAFT_IS_FULLTILE": 1 << 2,
    "CRAFT_CHECK_DIRECTION": 1 << 3,
    "CRAFT_ON_SOLID_GROUND": 1 << 4,
    "CRAFT_CHECK_DENSITY": 1 << 5,
    "CRAFT_TRANSFERS_REAGENT_COMPONENTS": 1 << 6,
    "CRAFT_CLEARS_REAGENTS": 1 << 7,
    "CRAFT_SKIP_MATERIALS_PARITY": 1 << 8,
    "CRAFT_IGNORE_DO_AFTER": 1 << 9,
    "CRAFT_NO_MATERIALS": 1 << 10,
    "CRAFT_COLLECT_REQUIREMENTS": 1 << 11,
    "NONE": 0,
}
PLACEMENT = {
    "STACK_CHECK_CARDINALS": 1 << 0,
    "STACK_CHECK_ADJACENT": 1 << 1,
    "STACK_CHECK_TRAM_FORBIDDEN": 1 << 2,
    "STACK_CHECK_TRAM_EXCLUSIVE": 1 << 3,
    "NONE": 0,
}
POSITIONAL = ["title", "result_type", "req_amount", "res_amount", "max_res_amount", "time",
              "crafting_flags", "placement_checks", "trait_booster", "trait_modifier", "category"]


def strip_comments(text):
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
        elif text.startswith("/*", i):
            i = text.find("*/", i) + 2
        else:
            out.append(c)
            i += 1
    return "".join(out).replace("\\\n", "\n")


def matching(text, start):
    """The index just past the bracket closing the one at `start`."""
    depth, i = 0, start
    while i < len(text):
        c = text[i]
        if c == '"':
            i += 1
            while text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise ValueError("unbalanced")


def split_args(body):
    args, depth, cur, i = [], 0, [], 0
    while i < len(body):
        c = body[i]
        if c == '"':
            j = i + 1
            while body[j] != '"':
                j += 2 if body[j] == "\\" else 1
            cur.append(body[i:j + 1])
            i = j + 1
            continue
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        if c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    if "".join(cur).strip():
        args.append("".join(cur).strip())
    return args


def flags(expr, table):
    value = 0
    for part in expr.split("|"):
        part = part.strip().strip("()")
        if part:
            value |= table[part]
    return value


def deciseconds(expr):
    expr = expr.strip()
    m = re.fullmatch(r"([\d.]+)\s*SECONDS", expr)
    if m:
        return float(m.group(1)) * 10
    m = re.fullmatch(r"([\d.]+)\s*DECISECONDS", expr)
    if m:
        return float(m.group(1))
    return float(expr)


def unquote(s):
    s = s.strip()
    assert s.startswith('"'), s
    return s[1:-1].replace('\\"', '"')


def parse_entries(body):
    """The entries of a `list(...)` body: recipes, sublists; `null` separators dropped."""
    entries = []
    for arg in split_args(body):
        if arg == "null" or not arg:
            continue
        m = re.match(r"new\s*/datum/stack_recipe_list\s*\(", arg)
        if m:
            inner = arg[m.end() - 1:]
            inner = inner[1:matching(inner, 0) - 1]
            title, rest = split_args(inner)[0], split_args(inner)[1]
            lm = re.match(r"list\s*\(", rest)
            sub = rest[lm.end() - 1:]
            entries.append(("list", unquote(title), parse_entries(sub[1:matching(sub, 0) - 1])))
            continue
        m = re.match(r"new\s*/datum/stack_recipe(/radial)?\s*\(", arg)
        assert m, arg
        inner = arg[m.end() - 1:]
        inner = inner[1:matching(inner, 0) - 1]
        values = {}
        for i, a in enumerate(split_args(inner)):
            nm = re.match(r"(\w+)\s*=\s*(.*)$", a, re.S)
            if nm and not a.startswith('"'):
                values[nm.group(1)] = nm.group(2).strip()
            else:
                values[POSITIONAL[i]] = a
        req = int(values.get("req_amount", "1"))
        entries.append(("recipe", {
            "title": unquote(values["title"]),
            "result": values["result_type"].strip(),
            "req": req,
            "res": int(values.get("res_amount", "1")),
            "max_res": int(values.get("max_res_amount", "1")),
            "time": deciseconds(values.get("time", "0")),
            "flags": flags(values.get("crafting_flags", "CRAFT_CHECK_DENSITY"), CRAFT),
            "placement": flags(values.get("placement_checks", "NONE"), PLACEMENT),
            "quick": float(values.get("trait_modifier", "1"))
            if values.get("trait_booster", "").strip() == "TRAIT_QUICK_BUILD" else 1.0,
        }))
    return entries


def dm_files(*roots):
    for root in roots:
        for d, _, files in os.walk(os.path.join(SOURCE, root)):
            for f in sorted(files):
                if f.endswith(".dm"):
                    yield os.path.join(d, f)


lists = {}
mains = {}  # stack path -> (calls parent, [list names])
for path in dm_files("code", "modular_bandastation"):
    text = open(path, encoding="utf-8", errors="replace").read()
    if "stack_recipe" not in text and "get_main_recipes" not in text:
        continue
    text = strip_comments(text)
    for m in re.finditer(r"GLOBAL_LIST_INIT\((\w+_recipes),\s*list\s*\(", text):
        if "stack_recipe" not in text[m.end():m.end() + 300] and m.group(1) not in ("sand_recipes",):
            continue
        start = m.end() - 1
        body = text[start + 1:matching(text, start) - 1]
        if "stack_recipe" not in body:
            continue
        lists[m.group(1)] = parse_entries(body)
    for m in re.finditer(r"^(/obj/item/stack[\w/]*)/get_main_recipes\(\)\s*\n((?:[ \t]+.*\n?)*)",
                         text, re.M):
        body = m.group(2)
        mains[m.group(1)] = ("..()" in body, re.findall(r"GLOB\.(\w+_recipes)", body))

# BandaStation `modular_bandastation/objects/_objects.dm`.
WOOD_TILES = [("Обычный", "", 1), ("Дубовый", "/oak", 1), ("Тёмный", "/darkwood", 1),
              ("Светлый", "/lightwood", 1), ("Берёзовый", "/birch", 1), ("Вишнёвый", "/cherry", 1),
              ("Амарантовый", "/amaranth", 1), ("Эбонитовый", "/ebonite", 2),
              ("Умниниевый", "/pink_ivory", 2), ("Бакаутовый", "/guaiacum", 2)]
wood = lists["wood_recipes"]
for i, e in enumerate(wood):
    if e[0] == "recipe" and e[1]["result"] == "/obj/item/stack/tile/wood":
        del wood[i]
        break
wood.append(("list", "Деревянный пол", [("recipe", {
    "title": t, "result": "/obj/item/stack/tile/wood" + s, "req": r, "res": 4, "max_res": 20,
    "time": 0.0, "flags": CRAFT["CRAFT_CHECK_DENSITY"], "placement": 0, "quick": 1.0,
}) for t, s, r in WOOD_TILES]))


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def num(f):
    return f"{float(f)!r}"


def recipe(r):
    return (f"r({rust_str(r['title'])}, {rust_str(r['result'])}, {r['req']}, {r['res']}, "
            f"{r['max_res']}, {num(r['time'])}, {r['flags']}, {r['placement']}, {num(r['quick'])})")


def entries(es, indent):
    pad = " " * indent
    out = []
    for e in es:
        if e[0] == "recipe":
            out.append(f"{pad}Entry::Recipe({recipe(e[1])}),")
        else:
            out.append(f"{pad}Entry::List(\n{pad}    {rust_str(e[1])},\n{pad}    &[")
            out.extend(f"{pad}        {recipe(r[1])}," for r in e[2] if r[0] == "recipe")
            out.append(f"{pad}    ],\n{pad}),")
    return "\n".join(out)


def resolved(stack):
    """`get_main_recipes` of the type: the nearest override, with its parents' when it calls them."""
    p = stack
    while p.startswith("/obj/item/stack"):
        if p in mains:
            parent, own = mains[p]
            up = p.rsplit("/", 1)[0]
            return (resolved(up) if parent and up.startswith("/obj/item/stack") else []) + own
        p = p.rsplit("/", 1)[0]
    return []


lines = [
    "// Generated by scripts/gen-stack-recipes.py from the BYOND checkout's DM source. Do not edit.",
    "",
    "use super::{Entry, r};",
    "",
]
for name in sorted(lists):
    lines.append(f"pub static {name.upper()}: &[Entry] = &[")
    lines.append(entries(lists[name], 4))
    lines.append("];")
    lines.append("")
lines.append("/// The stack types with a crafting menu (`get_main_recipes`), and its lists in order.")
lines.append("pub static STACKS: &[(&str, &[&[Entry]])] = &[")
for stack in sorted(mains):
    names = [n for n in resolved(stack) if n in lists]
    if names:
        refs = ", ".join(n.upper() for n in names)
        lines.append(f"    ({rust_str(stack)}, &[{refs}]),")
lines.append("];")
lines.append("")
open(OUT, "w").write("\n".join(lines) + "\n")
results = set()


def collect(es):
    for e in es:
        if e[0] == "recipe":
            results.add(e[1]["result"])
        else:
            collect(e[2])


for name in lists:
    collect(lists[name])
types = [
    "// Generated by scripts/gen-stack-recipes.py from the BYOND checkout's DM source. Do not edit.",
    "",
    "/// What the stack crafting menus make (`ss13_game::stack_recipes`).",
    "pub const STACK_RECIPE_RESULTS: &[&str] = &[",
] + [f"    {rust_str(p)}," for p in sorted(results)] + ["];", ""]
open(TYPES_OUT, "w").write("\n".join(types))
print(f"{len(lists)} lists, {sum(1 for s in mains if resolved(s))} stacks -> {OUT}")
