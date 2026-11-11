import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import set_version as s  # noqa: E402

PAGE = '''```sh [install.sh]
npm install --save-exact @fedimint/react-native@0.1.0-beta.2 @fedimint/react-native-bindings@0.1.0-beta.2
```
| `@fedimint/react-native`          | `0.1.0-beta.2` | Install the exact version |
| `@fedimint/react-native-bindings` | `0.1.0-beta.2`    | The same |
| React Native | `0.78` or newer |
react-native-fs@2.20.0 and @fedimint/react-native-bindings/app.plugin.js stay as they are.'''


class SetVersion(unittest.TestCase):
    def test_finds_every_pin(self):
        self.assertEqual(s.pins(PAGE), ['0.1.0-beta.2'] * 4)

    def test_rewrites_every_pin_and_nothing_else(self):
        out = s.rewrite(PAGE, '0.2.0')
        self.assertEqual(s.pins(out), ['0.2.0'] * 4)
        self.assertNotIn('0.1.0-beta.2', out)
        self.assertIn('`0.78` or newer', out)
        self.assertIn('react-native-fs@2.20.0', out)
        self.assertIn('@fedimint/react-native-bindings/app.plugin.js', out)

    def test_rewrite_is_idempotent(self):
        once = s.rewrite(PAGE, '0.1.0-beta.3')
        self.assertEqual(s.rewrite(once, '0.1.0-beta.3'), once)

    def test_grammar_matches_the_release_workflow(self):
        for good in ['0.1.0', '0.1.0-beta.1', '10.20.30-beta.400']:
            self.assertTrue(s.GRAMMAR.fullmatch(good), good)
        for bad in ['latest', 'beta', '^0.1.0', '0.1', '0.1.0-rc.1', 'v0.1.0']:
            self.assertFalse(s.GRAMMAR.fullmatch(bad), bad)

    def test_release_order(self):
        order = ['0.1.0-beta.1', '0.1.0-beta.2', '0.1.0-beta.10', '0.1.0', '0.1.1-beta.1', '0.2.0']
        self.assertEqual(sorted(reversed(order), key=s.release_key), order)

    def test_newest_release_reads_only_release_tags(self):
        tags = ['react-native-sdk-v0.1.0-beta.1', 'react-native-sdk-v0.1.0-beta.10',
                'react-native-sdk-v0.1.0-beta.2', 'android-sdk-v9.0.0', 'react-native-sdk-vbad']
        self.assertEqual(s.newest_release(tags), '0.1.0-beta.10')
        self.assertIsNone(s.newest_release(['android-sdk-v9.0.0']))


if __name__ == '__main__':
    unittest.main()
