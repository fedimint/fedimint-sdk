"""The extractor, run as the CLI is run, over Markdown pages."""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / 'tools' / 'extract_snippets.py'

PAGE = '''# Demo

### 01 · Hello {#step-01}

```ts [hello.ts]
export const hello = 1; // [!code focus]
```

```text no-check
output only
```
'''


SIGNATURE_PAGE = '''# API

{sig1}

{sig2}
'''


def run(*args):
    return subprocess.run([sys.executable, str(TOOL), *args], capture_output=True, text=True)


class ExtractOptions(unittest.TestCase):
    def test_writes_into_the_named_fixtures_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'demo.md').write_text(PAGE, encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertEqual(result.returncode, 0, result.stderr)
            out = fixtures / 'demo' / 'step-01' / 'hello.ts'
            self.assertEqual(out.read_text(encoding='utf-8'), 'export const hello = 1;\n')

    def test_check_passes_then_fails_on_drift(self):
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'demo.md').write_text(PAGE, encoding='utf-8')
            run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertEqual(run('--check', '--guide', str(guide), '--fixtures', str(fixtures)).returncode, 0)
            (fixtures / 'demo' / 'step-01' / 'hello.ts').write_text('changed\n', encoding='utf-8')
            self.assertNotEqual(run('--check', '--guide', str(guide), '--fixtures', str(fixtures)).returncode, 0)

    def test_signature_blocks_become_one_type_check(self):
        page = SIGNATURE_PAGE.format(
            sig1="<!-- signature: SdkLike['reopenFederation'] -->\n```ts\nreopenFederation(id: FederationId): Promise<FederationLike>\n```",
            sig2='<!-- signature: typeof Notes.parse -->\n```ts\nwatch(cb: (v: Amount) => void,\n  n?: number): Unsubscribe\n```',
        )
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'api.md').write_text(page, encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertEqual(result.returncode, 0, result.stderr)
            out = (fixtures / 'api' / 'step-00' / 'signatures.ts').read_text(encoding='utf-8')
            self.assertIn("import { Amount, FederationId, FederationLike, Notes, SdkLike, Unsubscribe } from '@fedimint/react-native'", out)
            self.assertIn("same<Same<(id: FederationId) => Promise<FederationLike>, SdkLike['reopenFederation']>>()", out)
            self.assertIn('same<Same<(cb: (v: Amount) => void, n?: number) => Unsubscribe, typeof Notes.parse>>()', out)

    def test_signature_target_can_name_a_guide_file(self):
        target = "typeof import('../../session/step-02/src/SessionContext').useSession"
        page = SIGNATURE_PAGE.format(
            sig1=f'<!-- signature: {target} -->\n```ts\nuseSession(): SdkSession\n```', sig2='')
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'api.md').write_text(page, encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertEqual(result.returncode, 0, result.stderr)
            out = (fixtures / 'api' / 'step-00' / 'signatures.ts').read_text(encoding='utf-8')
            self.assertIn("import { SdkSession } from '@fedimint/react-native'", out)
            self.assertIn(f'same<Same<() => SdkSession, {target}>>()', out)

    def test_signature_that_is_not_a_call_is_refused(self):
        page = SIGNATURE_PAGE.format(
            sig1="<!-- signature: SdkLike['join'] -->\n```ts\njoin: FederationLike\n```", sig2='')
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'api.md').write_text(page, encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('not a call signature', result.stderr)

    def test_unmarked_fence_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'demo.md').write_text('# Demo\n\n```ts\nconst x = 1\n```\n', encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('neither a [file] title nor no-check', result.stderr)

    def test_indented_fence_in_a_list_is_extracted(self):
        page = '# Demo\n\n### 02 · Two {#step-02}\n\n1. Write it:\n\n   ```tsx [App.tsx]\n   export {}\n   ```\n'
        with tempfile.TemporaryDirectory() as tmp:
            guide, fixtures = Path(tmp, 'g'), Path(tmp, 'f')
            guide.mkdir()
            (guide / 'demo.md').write_text(page, encoding='utf-8')
            result = run('--guide', str(guide), '--fixtures', str(fixtures))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((fixtures / 'demo' / 'step-02' / 'App.tsx').read_text(encoding='utf-8'), 'export {}\n')

    def test_option_without_a_path_is_refused(self):
        result = run('--guide')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('--guide needs a path', result.stderr)


if __name__ == '__main__':
    unittest.main()
