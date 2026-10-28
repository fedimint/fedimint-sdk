#!/usr/bin/env python3
"""Extract every code block in the React Native pages (react-native/*.md) into files under fixtures/.

The guide promises things about a package. This turns those promises into files a
compiler can read, so a promise that has gone stale fails a check instead of an
afternoon.

The contract is on the fence, and the pages carry it already:

    ```ts [path]                  this block is a file. Extract it to that path.
    ```text no-check              this block is output or a command, not checked. Skip it.
    <!-- signature: T -->         on the line before a fence: the block is a call signature,
    ```ts                         name(params): Result. The signatures of a page go into one
                                  file, signatures.ts, that tsc fails unless each one and the
                                  real type T accept each other.

A fence with none of these is an error, not a default. Silence is how a guide starts
lying: a block nobody decided about is a block nobody checks.

VitePress annotations such as `// [!code focus]` are for the reader. They are removed
from the extracted file.

Output is fixtures/react-native/<chapter>/step-NN/<path>, where the step number is read
from the {#step-NN} anchor of the heading above the block, so a fixture path and the
page's own numbering cannot disagree.

Run, from docs/:  python3 tools/extract_snippets.py [--check]

--check writes nothing and exits 1 if the extracted result would differ from what
is already on disk. That is the CI form: it fails when someone edits a fixture
instead of the page, or edits the page without re-extracting.
"""
from pathlib import Path
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
GUIDE = ROOT / 'react-native'
FIXTURES = ROOT / 'fixtures' / 'react-native'

FENCE = re.compile(r'^(\s*)(`{3,}|~{3,})\s*([^\s\[`]*)(.*)$')
STEP = re.compile(r'^#{2,6}\s.*\{#step-(\d+)\}\s*$')
SIGNATURE = re.compile(r'^\s*<!--\s*signature:\s*(.+?)\s*-->\s*$')
ANNOTATION = re.compile(r'\s*// \[!code [^\]]+\]')


class Block:
    """One extracted code block, and where the reader met it."""

    def __init__(self, chapter, step, path, text, lang):
        self.chapter = chapter
        self.step = step
        self.path = path
        self.text = text
        self.lang = lang

    @property
    def out(self):
        return FIXTURES / self.chapter / f'step-{self.step}' / self.path


class GuideParser:
    """Walks one page and collects its code blocks."""

    def __init__(self, chapter):
        self.chapter = chapter
        self.blocks = []
        self.signatures = []      # (step, target type, signature text)
        self.skipped = []
        self.errors = []

    def feed(self, text):
        step, signature, lines = '00', None, text.split('\n')
        i = 0
        while i < len(lines):
            line = lines[i]
            m = FENCE.match(line)
            if not m:
                s = STEP.match(line)
                if s:
                    step = s.group(1).zfill(2)
                sig = SIGNATURE.match(line)
                if sig:
                    signature = sig.group(1)
                elif line.strip():
                    signature = None
                i += 1
                continue
            pad, tick, lang, info = m.groups()
            body = []
            i += 1
            while i < len(lines) and not re.match(r'^\s*' + re.escape(tick) + r'\s*$', lines[i]):
                body.append(lines[i][len(pad):] if lines[i].startswith(pad) else lines[i].lstrip())
                i += 1
            i += 1
            self.close_block(step, lang, info, '\n'.join(body), signature)
            signature = None

    def close_block(self, step, lang, info, text, signature):
        text = '\n'.join(ANNOTATION.sub('', l) for l in text.split('\n'))
        if signature:
            self.signatures.append((step, signature, text))
            return
        if re.search(r'(^|\s)no-check(\s|$)', info):
            self.skipped.append((step, lang))
            return
        m = re.search(r'\[([^\]]+)\]', info)
        if not m:
            first = text.strip().splitlines()[0][:60] if text.strip() else '(empty)'
            self.errors.append(
                f'{self.chapter} step {step}: a fence has neither a [file] title nor no-check. '
                f'First line: {first}'
            )
            return
        path = m.group(1)
        if path.startswith('/') or '..' in Path(path).parts:
            self.errors.append(f'{self.chapter} step {step}: unsafe file title {path!r}')
            return
        self.blocks.append(Block(self.chapter, step, path, dedent_block(text), lang))


def dedent_block(text):
    """Strip the indentation the HTML gave the block, and normalise the edges.

    A code block is indented to sit nicely in the markup. That indentation is the
    page's, not the code's, so it comes off here. Only whole-block indentation is
    removed; relative indentation inside the block is what the reader copies.
    """
    lines = text.strip('\n').rstrip().split('\n')
    pad = [len(l) - len(l.lstrip(' ')) for l in lines if l.strip()]
    cut = min(pad) if pad else 0
    return '\n'.join(l[cut:] if l.strip() else '' for l in lines) + '\n'


SIGNATURE_MODULE = '@fedimint/react-native'
BUILTINS = {'Array', 'ReadonlyArray', 'Promise', 'Record', 'Partial', 'Readonly', 'Uint8Array',
            'AbortSignal', 'AsyncIterable', 'AsyncIterator', 'Iterable', 'Map', 'Set', 'Error'}


def arrow_type(signature):
    """name<T>(params): Result  ->  <T>(params) => Result, or None if it is not a call."""
    text = ' '.join(signature.split())
    m = re.match(r'[\w.]+\s*(<[^(]*>)?\s*\(', text)
    if not m:
        return None
    depth, start = 0, m.end() - 1
    for i in range(start, len(text)):
        depth += {'(': 1, ')': -1}.get(text[i], 0)
        if depth == 0:
            rest = text[i + 1:].strip()
            if not rest.startswith(':'):
                return None
            params = text[start + 1:i].strip().rstrip(',').replace('( ', '(').replace(' )', ')')
            return f"{m.group(1) or ''}({params}) => {rest[1:].strip().rstrip(';')}"
    return None


def signature_block(chapter, signatures, errors):
    """One file that tsc rejects if a signature on the page and its real type disagree.

    Each side must be assignable to the other. That allows the page to leave out a
    trailing optional parameter (the bindings' asyncOpts_), and nothing else.
    """
    # No semicolons, as the repository's Prettier config writes TypeScript.
    lines, names = [], set()
    for step, target, text in signatures:
        arrow = arrow_type(text)
        if arrow is None:
            errors.append(f'{chapter} step {step}: signature block is not a call signature: '
                          f'{" ".join(text.split())[:60]}')
            continue
        generics = set(re.findall(r'<\s*(\w+)', arrow.split('(')[0]))
        # typeof import('<path>').name names a type in a guide file, not in the package.
        local = re.sub(r"import\(\s*'[^']*'\s*\)\.\w+", '', target)
        found = re.findall(r'\b[A-Z]\w*', arrow + ' ' + local) + re.findall(r'typeof\s+(\w+)', local)
        names |= {n for n in found if n not in BUILTINS and n not in generics}
        lines.append(f'same<Same<{arrow}, {target}>>()')
    head = [
        '// Generated by tools/extract_snippets.py from the signature blocks. Do not edit.',
        f"import {{ {', '.join(sorted(names))} }} from '{SIGNATURE_MODULE}'",
        '',
        'type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false',
        'function same<T extends true>(): void {}',
        '',
    ]
    return Block(chapter, '00', 'signatures.ts', '\n'.join(head + lines) + '\n', 'typescript')


def extract():
    chapters = sorted(p for p in GUIDE.glob('*.md') if p.name != 'index.md')
    if not chapters:
        raise SystemExit(f'no pages in {GUIDE}, nothing to extract')

    blocks, skipped, errors = [], [], []
    for page in chapters:
        p = GuideParser(page.stem)
        p.feed(page.read_text(encoding='utf-8'))
        blocks += p.blocks
        if p.signatures:
            blocks.append(signature_block(page.stem, p.signatures, p.errors))
        skipped += [(page.stem, s, l) for s, l in p.skipped]
        errors += p.errors

    if errors:
        for e in errors:
            print(f'ERROR: {e}', file=sys.stderr)
        raise SystemExit(f'{len(errors)} block(s) opted into neither extraction nor exemption')

    seen = {}
    for b in blocks:
        if b.out in seen:
            raise SystemExit(f'two blocks both claim {b.out.relative_to(ROOT)}')
        seen[b.out] = b

    return blocks, skipped


def chapter_dirs():
    """The directories this tool owns: one per chapter, named after its file.

    Everything else in fixtures/ belongs to npm and to the fixture project, and is
    neither read nor written here.
    """
    if not FIXTURES.exists():
        return []
    names = {p.stem for p in GUIDE.glob('*.md') if p.name != 'index.md'}
    return sorted(FIXTURES / n for n in names if (FIXTURES / n).is_dir())


def read_or_none(path):
    """Text, or a marker that cannot equal any extracted block.

    A binary file under a chapter directory is drift by definition: nothing here
    produces one.
    """
    try:
        return path.read_text(encoding='utf-8')
    except UnicodeDecodeError:
        return None


def write(blocks, check_only):
    """Write the tree, or compare against it.

    The tree is rebuilt from nothing each time. A fixture with no block behind it
    any more is a fixture that stopped being checked against anything, so it is
    removed rather than left to rot.
    """
    wanted = {b.out: b.text for b in blocks}

    if check_only:
        # Only ever look inside the chapter directories this tool generates. The
        # rest of fixtures/ belongs to npm and is none of its business.
        have = {}
        for chapter in chapter_dirs():
            for path in sorted(chapter.rglob('*')):
                if path.is_file():
                    have[path] = read_or_none(path)

        drift = []
        for path, text in sorted(wanted.items()):
            try:
                rel = path.relative_to(ROOT)
            except ValueError:
                rel = path
            if path not in have:
                drift.append(f'missing: {rel}')
            elif have[path] != text:
                drift.append(f'differs from the guide: {rel}')
        for path in sorted(have):
            if path not in wanted:
                try:
                    rel = path.relative_to(ROOT)
                except ValueError:
                    rel = path
                drift.append(f'no block in the guide produces it: {rel}')
        if drift:
            for d in drift:
                print(f'DRIFT: {d}', file=sys.stderr)
            raise SystemExit('fixtures do not match the pages. Run tools/extract_snippets.py')
        print(f'fixtures match the guide: {len(wanted)} file(s)')
        return

    # Keep node_modules and the fixture project files; rebuild everything else.
    for child in sorted(FIXTURES.glob('*')) if FIXTURES.exists() else []:
        if child.is_dir() and child.name != 'node_modules':
            shutil.rmtree(child)

    for path, text in sorted(wanted.items()):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')


def option(argv, name):
    """The path after a flag, resolved against the repo root, or None if the flag is absent.

    An absolute path stays absolute, because joining an absolute path onto ROOT
    gives the absolute path back.
    """
    if name not in argv:
        return None
    i = argv.index(name)
    if i + 1 >= len(argv) or argv[i + 1].startswith('--'):
        raise SystemExit(f'{name} needs a path')
    return ROOT / argv[i + 1]


def main():
    global GUIDE, FIXTURES
    argv = sys.argv[1:]
    GUIDE = option(argv, '--guide') or GUIDE
    FIXTURES = option(argv, '--fixtures') or FIXTURES
    check_only = '--check' in argv
    blocks, skipped = extract()
    write(blocks, check_only)

    if check_only:
        return

    by_chapter = {}
    for b in blocks:
        by_chapter.setdefault(b.chapter, []).append(b)

    for chapter, items in sorted(by_chapter.items()):
        print(f'{chapter}')
        for b in sorted(items, key=lambda b: (b.step, b.path)):
            print(f'  step-{b.step}  {b.path:<40} {b.lang}')
    try:
        fixtures_rel = FIXTURES.relative_to(ROOT)
    except ValueError:
        fixtures_rel = FIXTURES
    print(f'\n{len(blocks)} extracted, {len(skipped)} exempt as output, '
          f'into {fixtures_rel}/')


if __name__ == '__main__':
    main()
