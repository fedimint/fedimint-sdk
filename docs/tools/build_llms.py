#!/usr/bin/env python3
"""Write the files an AI agent reads, from the React Native pages.

  public/react-native/md/<page>.md    each page as plain Markdown
  public/react-native/llms.txt        an index of the pages, per the llms.txt convention
  public/react-native/llms-full.txt   every page in one file, in nav order

The pages are VitePress Markdown. This removes what only VitePress reads: front matter,
heading anchors {#id}, attributes {.class}, <Badge> tags, line annotations // [!code ...],
the file title of a fence, and raw HTML figures. A fence's file title becomes a line above
the block, so the agent still knows which file it is.

The output is generated at build time (npm run build) and is not committed.

Run, from docs/:  python3 tools/build_llms.py
"""
from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parents[1]
PAGES = ROOT / 'react-native'
OUT = ROOT / 'public' / 'react-native'

BADGE = re.compile(r'<Badge type="(\w+)" text="([^"]*)" />')
STATE = {'tip': 'Proven', 'danger': 'Not yet', 'warning': 'Partly'}
FENCE = re.compile(r'^(\s*)(`{3,})(\S*)(.*)$')


def pages_of(nav):
    for g in nav['groups']:
        yield g, None, g.get('pages', [])
        for s in g.get('sections', []):
            yield g, s, s['pages']


def plain(text):
    """One page, as Markdown an agent can read without VitePress."""
    text = re.sub(r'\A---\n.*?\n---\n+', '', text, flags=re.S)
    text = re.sub(r'<figure.*?</figure>\n*', '', text, flags=re.S)
    out, in_fence = [], False
    for line in text.split('\n'):
        m = FENCE.match(line)
        if m:
            pad, tick, lang, info = m.groups()
            if not in_fence:
                title = re.search(r'\[([^\]]+)\]', info)
                if title and 'no-check' not in info:
                    out += [f'{pad}`{title.group(1)}`', '']
                out.append(f'{pad}{tick}{lang}')
            else:
                out.append(line)
            in_fence = not in_fence
            continue
        if in_fence:
            out.append(re.sub(r'\s*// \[!code [^\]]+\]', '', line))
            continue
        if re.match(r'^\s*<!--.*-->\s*$', line) or re.match(r'^\{\.[\w-]+\}$', line.strip()):
            continue
        if line.startswith('<p class="cap-row">'):
            out += [f'- {STATE.get(k, "Partly")}: {t}' for k, t in BADGE.findall(line)]
            continue
        line = re.sub(r'\s*<Badge [^>]*/>', '', line)
        line = re.sub(r'\s*\{#[\w-]+\}\s*$', '', line)
        line = re.sub(r'<span id="[^"]*"></span>', '', line)
        line = re.sub(r'\]\(\./([\w-]+\.md(?:#[\w-]*)?)\)', r'](\1)', line)
        if line.startswith(':::'):
            continue
        out.append(line)
    return re.sub(r'\n{3,}', '\n\n', '\n'.join(out)).strip() + '\n'


def build():
    nav = json.loads((PAGES / 'nav.json').read_text(encoding='utf-8'))
    out = {}
    full = []
    llms = ['# ' + nav['title'], '', '> ' + nav['summary']]
    current = None
    for group, section, pages in pages_of(nav):
        if group is not current:
            llms += ['', '## ' + group['title']]
            current = group
        llms += ['', '### ' + section['title']] if section else []
        llms.append('')
        for p in pages:
            md = plain((PAGES / f'{p["slug"]}.md').read_text(encoding='utf-8'))
            out[OUT / 'md' / f'{p["slug"]}.md'] = md
            full.append(md)
            llms.append(f'- [{p["title"]}](md/{p["slug"]}.md): {p.get("description", "")}')
    out[OUT / 'llms.txt'] = '\n'.join(llms) + '\n'
    out[OUT / 'llms-full.txt'] = f'# {nav["title"]}: the full guide\n\n' + '\n---\n\n'.join(full)
    return out


def main():
    files = build()
    for path, text in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
    print(f'{len(files)} file(s) into {OUT.relative_to(ROOT)}/')


if __name__ == '__main__':
    main()
