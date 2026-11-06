#!/usr/bin/env python3
"""Generates crates/ss13_game/src/service/manual_data.rs from BandaStation DM sources
(the SDSM-35 and IDC-27 tgui manuals).
usage: scripts/gen-manual-data.py <BandaStation dir> <out.rs>"""
import os, re, sys

src, out = sys.argv[1], sys.argv[2]
ROOTS = ("/datum/brain_trauma", "/datum/quirk", "/datum/addiction", "/datum/disease",
         "/datum/symptom", "/datum/reagent")
HEADER = re.compile(r'^(/datum/[\w/]+?)\s*(//.*)?$')
VAR = re.compile(r'^\t(?:var/(?:\w+/)*)?(\w+)\s*=\s*(.*)$')

types = {}

def parse_string(text):
    # text starts at a quote (or {" ); returns the decoded string or None
    if text.startswith('{"'):
        end = text.find('"}')
        body = text[2:end]
    elif text.startswith('"'):
        i, body = 1, ''
        while i < len(text):
            c = text[i]
            if c == '\\' and i + 1 < len(text):
                body += text[i:i + 2]; i += 2; continue
            if c == '"':
                break
            body += c; i += 1
    else:
        return None
    body = body.replace('\\improper ', '').replace('\\proper ', '').replace('\\improper', '')
    body = body.replace('\\"', '"').replace('\\n', '\n').replace('\\[', '[').replace('\\]', ']')
    body = body.replace('\\\\', '\\')
    return body

for base in ("code",):
    for root, _, files in os.walk(os.path.join(src, base)):
        for f in sorted(files):
            if not f.endswith('.dm'):
                continue
            lines = open(os.path.join(root, f), encoding='utf-8', errors='replace').read().split('\n')
            cur, i = None, 0
            while i < len(lines):
                line = lines[i].rstrip('\r')
                i += 1
                m = HEADER.match(line)
                if m and m.group(1).startswith(ROOTS) and '(' not in m.group(1):
                    cur = m.group(1).rstrip('/')
                    types.setdefault(cur, {})
                    continue
                if line and not line[0].isspace():
                    cur = None
                    continue
                if cur is None:
                    continue
                v = VAR.match(line)
                if not v:
                    continue
                name, value = v.group(1), v.group(2)
                if value.startswith('{"'):
                    while '"}' not in value and i < len(lines):
                        value += '\n' + lines[i]; i += 1
                else:
                    while value.rstrip().endswith('\\') and i < len(lines):
                        value = value.rstrip()[:-1] + lines[i].strip(); i += 1
                types[cur][name] = value.strip()

def parent(p):
    return p.rsplit('/', 1)[0]

def raw(p, var):
    while p.count('/') >= 2:
        if var in types.get(p, {}):
            return types[p][var]
        p = parent(p)
    return None

def split_plus(v):
    parts, cur, q = [], '', False
    i = 0
    while i < len(v):
        c = v[i]
        if c == '\\' and q:
            cur += v[i:i + 2]; i += 2; continue
        if c == '"':
            q = not q
        if c == '+' and not q:
            parts.append(cur.strip()); cur = ''
        else:
            cur += c
        i += 1
    parts.append(cur.strip())
    return parts

def text(p, var):
    v = raw(p, var)
    if v is None:
        return ''
    out = ''
    for part in split_plus(v.split(' //')[0] if not v.startswith('{"') else v):
        m = re.match(r'^(/datum/[\w/]+)::(\w+)$', part)
        if m:
            out += text(m.group(1), m.group(2))
            continue
        s = parse_string(part)
        if s is None:
            return out
        out += s
    return out

def subtypes(root):
    return sorted(p for p in types if p.startswith(root + '/'))

def valid(root):
    return [p for p in subtypes(root) if types[p].get('abstract_type', '').split('//')[0].strip() != p]

def flag_on(p, var, flag):
    v = raw(p, var)
    return v is not None and flag in v

def false(p, var):
    v = raw(p, var)
    return v is not None and v.split('//')[0].strip() in ('FALSE', '0')

dsm = []
for p in valid('/datum/brain_trauma'):
    if p.startswith('/datum/brain_trauma/mild/phobia'):
        continue
    if false(p, 'known_trauma'):
        continue
    dsm.append((text(p, 'name'), text(p, 'scan_desc'), text(p, 'desc'), text(p, 'symptoms'), p))
for p in valid('/datum/quirk'):
    if not flag_on(p, 'quirk_flags', 'QUIRK_TRAUMALIKE'):
        continue
    dsm.append((text(p, 'name'), text(p, 'name').lower(), text(p, 'medical_record_text'),
                text(p, 'medical_symptom_text'), p))
for p in subtypes('/datum/addiction'):
    n = text(p, 'name')
    dsm.append((f"{n[:1].upper()}{n[1:]} Addiction", f"{n} addiction", text(p, 'description'),
                text(p, 'symptoms'), p))
phobia_src = open(os.path.join(src, 'code/_globalvars/phobias.dm'), encoding='utf-8').read()
block = phobia_src.split('GLOBAL_LIST_INIT(phobia_types, list(')[1].split('))')[0]
for key, val in re.findall(r'"([^"]+)"\s*=\s*"([^"]+)"', block):
    root = '/datum/brain_trauma/mild/phobia'
    dsm.append((val, f"{text(root, 'scan_desc')} of {key}", f"Patient is irrationally afraid of {key}.",
                text(root, 'symptoms'), f"{root}/{key}"))

def spread_text(flags):
    # get_disease_spread_text
    out = []
    for flag, name in (('DISEASE_SPREAD_SPECIAL', 'Special'),
                       ('DISEASE_SPREAD_NON_CONTAGIOUS', 'Non-contagious'),
                       ('DISEASE_SPREAD_BLOOD', 'Blood'),
                       ('DISEASE_SPREAD_CONTACT_FLUIDS', 'Fluids'),
                       ('DISEASE_SPREAD_CONTACT_SKIN', 'Skin contact'),
                       ('DISEASE_SPREAD_AIRBORNE', 'Airborne')):
        if flags and flag in flags:
            out.append(name)
    return ', '.join(out)

idc = []
for p in valid('/datum/disease'):
    if flag_on(p, 'visibility_flags', 'HIDDEN_BOOK'):
        continue
    idc.append((text(p, 'form'), text(p, 'agent'), text(p, 'name'), text(p, 'desc'), 'N/A',
                text(p, 'spread_text') or spread_text(raw(p, 'spread_flags')),
                text(p, 'cure_text'), p, 'N/A'))
for p in valid('/datum/symptom'):
    cure = (raw(p, 'symptom_cure') or '').split('//')[0].strip()
    cure_name = text(cure, 'name') if cure.startswith('/datum/reagent') else ''
    level = (raw(p, 'level') or '0').split('//')[0].strip()
    idc.append(('Symptom', 'N/A', text(p, 'name'), text(p, 'desc'), text(p, 'illness'), 'N/A',
                cure_name, p, level))

def rs(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n') + '"'

with open(out, 'w', encoding='utf-8') as o:
    o.write('// Generated from BandaStation `code/` (`/obj/item/tgui_book/manual/dsm` and `idc`\n')
    o.write('// `ui_static_data`: brain traumas, trauma-like quirks, addictions, `phobia_types`;\n')
    o.write('// diseases without `HIDDEN_BOOK`, symptoms). Keep in sync by regenerating, not by hand.\n\n')
    o.write('/// `full_name`, `scan_name`, `desc`, `symptoms`, `id`.\n')
    o.write(f'pub static DSM: [[&str; 5]; {len(dsm)}] = [\n')
    for row in dsm:
        o.write('    [' + ', '.join(rs(x) for x in row) + '],\n')
    o.write('];\n\n')
    o.write('/// `form`, `agent`, `name`, `desc`, `illness`, `spread_by`, `cured_by`, `id`, `level`\n')
    o.write('/// (the advanced disease entry is added by the book).\n')
    o.write(f'pub static IDC: [[&str; 9]; {len(idc)}] = [\n')
    for row in idc:
        o.write('    [' + ', '.join(rs(x) for x in row) + '],\n')
    o.write('];\n')
print(len(dsm), len(idc))
