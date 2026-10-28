#!/usr/bin/env python3
"""Set the package version the guide pins, on every page, in one step.

The guide installs `@fedimint/react-native` and `@fedimint/react-native-bindings` at one exact
version. That version lives in `VERSION`. The pages write it literally in two places:

- install commands: `@fedimint/react-native@X` and `@fedimint/react-native-bindings@X`
- the version table on the Installation page: a row that names a package, then `` `X` ``

Usage, from docs/:

    python3 tools/set_version.py 0.1.0-beta.3   # rewrite VERSION, the pages, then rebuild
    python3 tools/set_version.py --check        # fail if a page pins another version, or VERSION
                                                # is older than the newest react-native-sdk-v* tag

After a rewrite it runs tools/build_llms.py and tools/extract_snippets.py, so the derived
files and the fixtures match the pages.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TAG_PREFIX = 'react-native-sdk-v'
SITE = ROOT / 'react-native'
VERSION_FILE = SITE / 'VERSION'
CONVENTIONS = SITE / 'CONVENTIONS.md'

# The grammar the release workflow accepts: X.Y.Z or X.Y.Z-beta.N.
GRAMMAR = re.compile(r'\d+\.\d+\.\d+(?:-beta\.\d+)?')
SEMVER = r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?'
# An install spec: the package name, `@`, a version.
SPEC = re.compile(r'(@fedimint/react-native(?:-bindings)?@)(' + SEMVER + r')')
# A Markdown table row that names one of the packages, then the version in a code cell.
# Prettier pads the cells, so the spaces around them vary.
ROW = re.compile(r'(\|\s*`@fedimint/react-native(?:-bindings)?`\s*\|\s*`)(' + SEMVER + r')(`)')


def pins(text):
    """Every version the text pins for the two packages."""
    return [m.group(2) for m in SPEC.finditer(text)] + [m.group(2) for m in ROW.finditer(text)]


def rewrite(text, version):
    """The text with every pin set to `version`."""
    text = SPEC.sub(lambda m: m.group(1) + version, text)
    return ROW.sub(lambda m: m.group(1) + version + m.group(3), text)


def pages():
    return sorted(SITE.glob('*.md'))


def check(version):
    """The pages that pin a version other than `version`, with what they pin."""
    wrong = []
    for page in pages():
        other = sorted({v for v in pins(page.read_text()) if v != version})
        if other:
            wrong.append(f'{page.relative_to(ROOT)}: {", ".join(other)}')
    return wrong


def release_key(version):
    """Sort key for a release version: a beta sorts before its release."""
    core, _, beta = version.partition('-beta.')
    return (*map(int, core.split('.')), 0 if beta else 1, int(beta or 0))


def newest_release(tags):
    """The newest version among the release tags, or None."""
    versions = [t[len(TAG_PREFIX):] for t in tags if t.startswith(TAG_PREFIX)]
    versions = [v for v in versions if GRAMMAR.fullmatch(v)]
    return max(versions, key=release_key, default=None)


def release_tags():
    out = subprocess.run(['git', 'tag', '-l', TAG_PREFIX + '*'], cwd=ROOT,
                         capture_output=True, text=True, check=True)
    return out.stdout.split()


def run(*args):
    subprocess.run([sys.executable, *args], cwd=ROOT, check=True)


def main(argv):
    current = VERSION_FILE.read_text().strip()
    if argv == ['--check']:
        wrong = check(current)
        if wrong:
            raise SystemExit(f'pages disagree with VERSION ({current}):\n  ' + '\n  '.join(wrong))
        count = sum(len(pins(p.read_text())) for p in pages())
        print(f'every pin is {current}: {count} pin(s)')
        newest = newest_release(release_tags())
        if newest is None:
            raise SystemExit(f'no {TAG_PREFIX}* tag found: fetch the tags, then run again')
        if release_key(current) < release_key(newest):
            raise SystemExit(f'VERSION ({current}) is older than the newest release ({newest}): '
                             f'run tools/set_version.py {newest}')
        print(f'VERSION is not older than the newest release ({newest})')
        return
    if len(argv) != 1 or argv[0].startswith('-'):
        raise SystemExit(__doc__)
    version = argv[0]
    if not GRAMMAR.fullmatch(version):
        raise SystemExit(f'not a release version (X.Y.Z or X.Y.Z-beta.N): {version}')

    changed = 0
    for page in pages():
        text = page.read_text()
        new = rewrite(text, version)
        if new != text:
            page.write_text(new)
            changed += 1
    VERSION_FILE.write_text(version + '\n')
    if CONVENTIONS.exists():
        text = CONVENTIONS.read_text()
        CONVENTIONS.write_text(text.replace(f'(today `{current}`)', f'(today `{version}`)'))
    print(f'{current} -> {version}: {changed} page(s) changed')

    run('tools/build_llms.py')
    run('tools/extract_snippets.py')
    wrong = check(version)
    if wrong:
        raise SystemExit('pins left behind:\n  ' + '\n  '.join(wrong))


if __name__ == '__main__':
    main(sys.argv[1:])
