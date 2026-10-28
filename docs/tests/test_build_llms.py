"""build_llms.plain: a VitePress page becomes Markdown an agent can read."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import build_llms as b  # noqa: E402

PAGE = '''---
title: "Show a balance"
---

# Show a balance

Read it once.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="ships" /> <Badge type="danger" text="no retry" /></p>

### 01 · Read it <Badge type="info" text="JavaScript" /> {#step-01}

See [Amounts](./amounts.md#units).

<!-- signature: SdkLike['join'] -->
```ts [read-balance.ts]
const msat = await fed.balance() // [!code focus]
```

::: tip Check
You will see `0 sats`.
:::

<figure class="flow-figure"><svg></svg></figure>

| <span id="code-timeout"></span>`Timeout` | slow |
'''


class Plain(unittest.TestCase):
    def setUp(self):
        self.out = b.plain(PAGE)

    def test_removes_what_only_vitepress_reads(self):
        for gone in ['---', 'title:', '{.subtitle}', '{#step-01}', '<Badge', '[!code', ':::',
                     '<!--', '<figure', '<span id=', '[read-balance.ts]']:
            self.assertNotIn(gone, self.out, gone)

    def test_keeps_the_content(self):
        self.assertTrue(self.out.startswith('# Show a balance\n\nRead it once.\n'))
        self.assertIn('- Proven: ships\n- Not yet: no retry', self.out)
        self.assertIn('### 01 · Read it\n', self.out)
        self.assertIn('`read-balance.ts`\n\n```ts\nconst msat = await fed.balance()\n```', self.out)
        self.assertIn('You will see `0 sats`.', self.out)
        self.assertIn('| `Timeout` | slow |', self.out)

    def test_links_point_at_the_markdown_copies(self):
        self.assertIn('[Amounts](amounts.md#units)', self.out)


if __name__ == '__main__':
    unittest.main()
