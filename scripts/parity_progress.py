#!/usr/bin/env python3
"""Writes docs/parity/PROGRESS.html: readiness of every area, file and entry of the parity
catalog, gameplay areas first. Run by `scripts/parity-count.py`; standalone:
`python3 scripts/parity_progress.py`.

Readiness = (✅ + ½·🟡) / entries. The page is self-contained (no network), opens from disk.
"""
import datetime
import html
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "parity"
STATUS = re.compile(r"^- \*\*Статус в ss13-rs:\*\* (✅|🟡|❌)", re.M)
NON_GAMEPLAY = {"admin", "persistence-sql", "server", "map", "bandastation"}
KIND = {"✅": "done", "🟡": "part", "❌": "none"}
WORD = {"done": "готово", "part": "частично", "none": "нет"}


def readiness(c: dict) -> float:
    n = c["done"] + c["part"] + c["none"]
    return (c["done"] + c["part"] / 2) / n if n else 0.0


def area_title(folder: pathlib.Path) -> str:
    title = (folder / "README.md").read_text(encoding="utf-8").splitlines()[0].lstrip("# ")
    return re.sub(r"\s*\([a-z-]+\)$", "", title).split(":")[0].strip()


def load() -> list:
    areas = []
    for folder in sorted(p for p in ROOT.iterdir() if p.is_dir()):
        readme = (folder / "README.md").read_text(encoding="utf-8")
        descs = dict(re.findall(r"^\| \[([\w.-]+\.md)\]\([\w.-]+\.md\) \| (.+?) \|", readme, re.M))
        subs = []
        for f in sorted(folder.glob("*.md")):
            if f.name == "README.md":
                continue
            text = f.read_text(encoding="utf-8")
            title = re.search(r"^# (.+)", text, re.M)
            entries = []
            for block in re.split(r"\n(?=### )", text):
                if block.startswith("### "):
                    st = STATUS.search(block)
                    note = re.search(r"\*\*Статус в ss13-rs:\*\* \S+ (.*)", block)
                    entries.append({
                        "title": block.splitlines()[0][4:].strip(),
                        "kind": KIND.get(st.group(1), "none") if st else "none",
                        "note": (note.group(1).strip() if note else ""),
                    })
            counts = {k: sum(e["kind"] == k for e in entries) for k in WORD}
            subs.append({"title": title.group(1) if title else f.stem, "file": f"{folder.name}/{f.name}",
                         "desc": descs.get(f.name, ""), "entries": entries, **counts})
        counts = {k: sum(s[k] for s in subs) for k in WORD}
        areas.append({"folder": folder.name, "title": area_title(folder),
                      "gameplay": folder.name not in NON_GAMEPLAY, "subs": subs, **counts})
    return areas


def pct(c: dict) -> str:
    return f"{round(100 * readiness(c))}%"


def stack(c: dict, cls: str = "stack") -> str:
    n = max(c["done"] + c["part"] + c["none"], 1)
    segs = "".join(
        f'<i class="s-{k}" style="width:{100 * c[k] / n:.2f}%"></i>' for k in WORD if c[k]
    )
    return f'<span class="{cls}" aria-hidden="true">{segs}</span>'


def counts(c: dict) -> str:
    return (f'<span class="nums"><b class="t-done">{c["done"]}</b>'
            f'<b class="t-part">{c["part"]}</b><b class="t-none">{c["none"]}</b></span>')


def esc(s: str) -> str:
    return html.escape(s, quote=True)


def md_inline(s: str) -> str:
    """`code` spans of a status note; everything else as text."""
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", esc(s))


def render_area(a: dict) -> str:
    subs = sorted(a["subs"], key=lambda s: -readiness(s))
    n = a["done"] + a["part"] + a["none"]
    rows = []
    for s in subs:
        sn = s["done"] + s["part"] + s["none"]
        items = "".join(
            f'<li class="entry" data-text="{esc((e["title"] + " " + e["note"]).lower())}">'
            f'<span class="chip c-{e["kind"]}">{WORD[e["kind"]]}</span>'
            f'<span class="e-title">{esc(e["title"])}</span>'
            + (f'<span class="e-note">{md_inline(e["note"])}</span>' if e["note"] else "")
            + "</li>"
            for e in s["entries"]
        )
        rows.append(
            f'<details class="sub" data-text="{esc((s["title"] + " " + s["desc"]).lower())}">'
            f'<summary><span class="s-name"><span class="s-title">{esc(s["title"])}</span>'
            f'<span class="s-desc">{esc(s["desc"])}</span></span>'
            f'{stack(s, "stack mini")}<span class="s-n">{sn}</span>{counts(s)}'
            f'<span class="ready">{pct(s)}</span></summary>'
            f'<ul class="entries">{items}</ul>'
            f'<p class="src">Записи: <code>docs/parity/{esc(s["file"])}</code></p></details>'
        )
    return (
        f'<details class="area" id="{esc(a["folder"])}" data-ready="{readiness(a):.4f}" '
        f'data-size="{n}" data-name="{esc(a["title"])}" data-text="{esc(a["title"].lower())}">'
        f'<summary><span class="a-name">{esc(a["title"])}</span>{stack(a)}'
        f'<span class="a-n">{n}</span>{counts(a)}<span class="ready big">{pct(a)}</span></summary>'
        f'<div class="subs">{"".join(rows)}</div></details>'
    )


def commit() -> str:
    try:
        return subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True,
                              text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "?"


def main() -> None:
    areas = load()
    game = [a for a in areas if a["gameplay"]]
    other = [a for a in areas if not a["gameplay"]]
    total = lambda group: {k: sum(a[k] for a in group) for k in WORD}
    g, every = total(game), total(areas)
    gn = sum(g.values())
    order = lambda group: sorted(group, key=lambda a: -readiness(a))
    page = TEMPLATE.format(
        commit=esc(commit()),
        date=datetime.date.today().isoformat(),
        g_ready=pct(g),
        g_stack=stack(g, "stack wide"),
        g_n=gn,
        g_done=g["done"], g_part=g["part"], g_none=g["none"],
        all_ready=pct(every), all_n=sum(every.values()),
        game="".join(render_area(a) for a in order(game)),
        other="".join(render_area(a) for a in order(other)),
        groups=len(areas), files=sum(len(a["subs"]) for a in areas),
    )
    (ROOT / "PROGRESS.html").write_text(page, encoding="utf-8")


TEMPLATE = """<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Паритет space-station-rs</title>
<!-- Generated by scripts/parity_progress.py from docs/parity; do not edit by hand. -->
<style>
/* Layout: one reading column; areas as rows with a stacked bar, opening into files, then entries. */
:root {{
  --bg: #f3f5f2; --panel: #ffffff; --fg: #18201c; --muted: #5f6a64; --line: #d9dfda;
  --done: #2e8a57; --part: #c8901f; --none: #cfd6d1; --accent: #24638a; --hover: #eef2ee;
  --sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", sans-serif;
  --mono: ui-monospace, "SFMono-Regular", "JetBrains Mono", Menlo, Consolas, monospace;
  color-scheme: light;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #101412; --panel: #171d1a; --fg: #e3e9e5; --muted: #95a199; --line: #28312c;
    --done: #45b077; --part: #e0a83f; --none: #36403b; --accent: #73b3da; --hover: #1d2521;
    color-scheme: dark;
  }}
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--bg); color: var(--fg); font: 15px/1.5 var(--sans); }}
.wrap {{ max-width: 1080px; margin: 0 auto; padding: 40px max(16px, env(safe-area-inset-right)) 64px max(16px, env(safe-area-inset-left)); }}
header h1 {{ margin: 0 0 4px; font-size: 30px; letter-spacing: -0.01em; text-wrap: balance; }}
header p {{ margin: 0; color: var(--muted); }}
code {{ font-family: var(--mono); font-size: 0.88em; }}
.summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr)); gap: 16px; margin: 28px 0 20px; }}
.card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; }}
.card .label {{ font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); }}
.card .value {{ font-size: 44px; font-weight: 650; line-height: 1.1; margin: 4px 0 10px; font-variant-numeric: tabular-nums; }}
.card .detail {{ color: var(--muted); font-size: 13px; margin-top: 8px; }}
.legend {{ display: flex; flex-wrap: wrap; gap: 14px; font-size: 13px; color: var(--muted); align-items: center; }}
.legend i {{ display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 6px; vertical-align: -1px; }}
.controls {{ display: flex; flex-wrap: wrap; gap: 10px 16px; align-items: center; margin: 18px 0 14px; }}
.controls input[type=search] {{ flex: 1 1 260px; min-width: 0; padding: 9px 12px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); color: var(--fg); font: inherit; }}
.controls select {{ padding: 8px 10px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); color: var(--fg); font: inherit; }}
.controls label {{ color: var(--muted); font-size: 14px; display: flex; gap: 6px; align-items: center; }}
:focus-visible {{ outline: 2px solid var(--accent); outline-offset: 2px; }}
h2 {{ font-size: 13px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); margin: 26px 0 8px; font-weight: 600; }}
.list {{ background: var(--panel); border: 1px solid var(--line); border-radius: 10px; overflow: hidden; }}
details > summary {{ list-style: none; cursor: pointer; }}
details > summary::-webkit-details-marker {{ display: none; }}
.area > summary, .sub > summary {{ display: grid; align-items: center; gap: 14px; }}
.area > summary {{ grid-template-columns: 14px minmax(0, 1.4fr) minmax(80px, 1fr) 44px 132px 52px; padding: 12px 16px; border-top: 1px solid var(--line); }}
.list > .area:first-child > summary {{ border-top: 0; }}
.area > summary::before, .sub > summary::before {{ content: "▸"; color: var(--muted); transition: transform .15s; }}
.area[open] > summary::before, .sub[open] > summary::before {{ transform: rotate(90deg); }}
.area > summary:hover, .sub > summary:hover {{ background: var(--hover); }}
.a-name {{ font-weight: 600; min-width: 0; overflow-wrap: anywhere; }}
.a-n, .s-n {{ color: var(--muted); font-size: 13px; text-align: right; font-variant-numeric: tabular-nums; }}
.stack {{ display: flex; height: 10px; border-radius: 5px; overflow: hidden; background: var(--none); }}
.stack.wide {{ height: 14px; border-radius: 7px; }}
.stack.mini {{ height: 6px; }}
.stack i {{ display: block; height: 100%; }}
.s-done, .legend .s-done {{ background: var(--done); }}
.s-part, .legend .s-part {{ background: var(--part); }}
.s-none, .legend .s-none {{ background: var(--none); }}
.nums {{ display: flex; gap: 10px; justify-content: flex-end; font-variant-numeric: tabular-nums; font-size: 13px; }}
.nums b {{ font-weight: 600; min-width: 28px; text-align: right; }}
.t-done {{ color: var(--done); }} .t-part {{ color: var(--part); }} .t-none {{ color: var(--muted); }}
.ready {{ text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }}
.ready.big {{ font-size: 16px; }}
.subs {{ border-top: 1px solid var(--line); background: var(--bg); }}
.sub > summary {{ grid-template-columns: 12px minmax(0, 1.6fr) minmax(60px, 0.8fr) 40px 132px 52px; padding: 9px 16px 9px 34px; border-top: 1px solid var(--line); font-size: 14px; }}
.subs > .sub:first-child > summary {{ border-top: 0; }}
.s-name {{ display: flex; flex-direction: column; min-width: 0; }}
.s-title {{ font-weight: 550; }}
.s-desc {{ color: var(--muted); font-size: 12.5px; overflow-wrap: anywhere; }}
.entries {{ list-style: none; margin: 0; padding: 4px 16px 6px 62px; }}
.entry {{ display: grid; grid-template-columns: 78px minmax(0, 1fr); gap: 2px 10px; padding: 6px 0; border-top: 1px dashed var(--line); font-size: 13.5px; }}
.entry:first-child {{ border-top: 0; }}
.chip {{ grid-row: span 2; align-self: start; font-size: 11.5px; padding: 1px 8px; border-radius: 999px; text-align: center; font-weight: 600; }}
.c-done {{ background: color-mix(in srgb, var(--done) 18%, transparent); color: var(--done); }}
.c-part {{ background: color-mix(in srgb, var(--part) 20%, transparent); color: var(--part); }}
.c-none {{ background: color-mix(in srgb, var(--none) 55%, transparent); color: var(--muted); }}
.e-title {{ font-weight: 550; }}
.e-note {{ color: var(--muted); font-size: 12.5px; overflow-wrap: anywhere; }}
.src {{ margin: 0; padding: 0 16px 10px 62px; color: var(--muted); font-size: 12px; }}
.other .list {{ opacity: .82; }}
.empty {{ color: var(--muted); padding: 16px; }}
footer {{ margin-top: 28px; color: var(--muted); font-size: 12.5px; }}
@media (max-width: 720px) {{
  .area > summary {{ grid-template-columns: 12px minmax(0, 1fr) 52px; }}
  .sub > summary {{ grid-template-columns: 10px minmax(0, 1fr) 46px; padding-left: 22px; }}
  .area > summary .stack, .area > summary .a-n, .area > summary .nums,
  .sub > summary .stack, .sub > summary .s-n, .sub > summary .nums {{ display: none; }}
  .entries, .src {{ padding-left: 22px; }}
}}
@media (prefers-reduced-motion: reduce) {{ * {{ transition: none !important; }} }}
</style>
</head>
<body>
<div class="wrap">
<header>
  <h1>Паритет space-station-rs</h1>
  <p>Сколько механик BandaStation перенесено, по группам и подгруппам каталога <code>docs/parity</code>. Коммит <code>{commit}</code>, {date}.</p>
</header>

<section class="summary">
  <div class="card">
    <div class="label">Игровые области</div>
    <div class="value">{g_ready}</div>
    {g_stack}
    <div class="detail">{g_n} записей: {g_done} готово, {g_part} частично, {g_none} нет</div>
  </div>
  <div class="card">
    <div class="label">Как считается</div>
    <p style="margin:6px 0 10px">Готовность = (готово + ½ частично) / все записи. Запись — одна механика, семейство объектов или окно оригинала.</p>
    <div class="legend"><span><i class="s-done"></i>готово</span><span><i class="s-part"></i>частично</span><span><i class="s-none"></i>нет</span></div>
    <div class="detail">Весь каталог вместе с неигровыми областями: {all_ready} из {all_n} записей.</div>
  </div>
</section>

<div class="controls">
  <input type="search" id="q" placeholder="Поиск: группа, подгруппа или механика (например, «стол», «генокрад»)" aria-label="Поиск по каталогу">
  <label for="sort">Порядок
    <select id="sort">
      <option value="ready-desc">готовые сверху</option>
      <option value="ready-asc">отстающие сверху</option>
      <option value="size">по числу записей</option>
      <option value="name">по названию</option>
    </select>
  </label>
  <label><input type="checkbox" id="other" checked> неигровые области</label>
</div>

<h2>Игровые области</h2>
<div class="list" id="game">{game}</div>
<section class="other" id="other-sec">
<h2>Неигровые области — низкий приоритет</h2>
<div class="list" id="other-list">{other}</div>
</section>
<p class="empty" id="nothing" hidden>Ничего не найдено.</p>

<footer>{groups} групп, {files} подгрупп. Страница собирается командой <code>python3 scripts/parity-count.py</code>; правьте записи каталога, а не её.</footer>
</div>
<script>
(() => {{
  const q = document.getElementById('q');
  const sort = document.getElementById('sort');
  const other = document.getElementById('other');
  const lists = [document.getElementById('game'), document.getElementById('other-list')];
  const orders = {{
    'ready-desc': (a, b) => b.dataset.ready - a.dataset.ready,
    'ready-asc': (a, b) => a.dataset.ready - b.dataset.ready,
    'size': (a, b) => b.dataset.size - a.dataset.size,
    'name': (a, b) => a.dataset.name.localeCompare(b.dataset.name, 'ru'),
  }};
  sort.addEventListener('change', () => {{
    for (const list of lists) {{
      [...list.children].sort(orders[sort.value]).forEach((el) => list.appendChild(el));
    }}
  }});
  other.addEventListener('change', () => {{
    document.getElementById('other-sec').hidden = !other.checked;
    filter();
  }});
  const filter = () => {{
    const term = q.value.trim().toLowerCase();
    let shown = 0;
    for (const area of document.querySelectorAll('.area')) {{
      if (area.closest('#other-sec') && !other.checked) continue;
      const areaHit = !term || area.dataset.text.includes(term);
      let any = false;
      for (const sub of area.querySelectorAll('.sub')) {{
        const subHit = areaHit || sub.dataset.text.includes(term);
        let entryHit = false;
        for (const e of sub.querySelectorAll('.entry')) {{
          const hit = subHit || e.dataset.text.includes(term);
          e.hidden = !hit;
          entryHit ||= hit;
        }}
        sub.hidden = !(subHit || entryHit);
        if (term && !subHit && entryHit) sub.open = true;
        if (!term) sub.open = false;
        any ||= !sub.hidden;
      }}
      area.hidden = !(areaHit || any);
      if (term) area.open = !area.hidden && !areaHit;
      else area.open = false;
      shown += area.hidden ? 0 : 1;
    }}
    document.getElementById('nothing').hidden = shown > 0;
  }};
  q.addEventListener('input', filter);
}})();
</script>
</body>
</html>
"""


if __name__ == "__main__":
    main()
