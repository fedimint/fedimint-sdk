---
name: fedimint-react-native-install
description: Use when adding the Fedimint React Native SDK (@fedimint/react-native and @fedimint/react-native-bindings) to an existing or new React Native app for the first time. Gives pre-flight checks, the decisions an existing app needs, the safety rules for seed words and money, a failure table keyed on exact messages, and a definition of done.
---

# Install the Fedimint React Native SDK in an app

The guide is the source of truth. This skill tells you how to use it in a real app, and what
an earlier install found that the pages do not make obvious. Read the guide pages in order:
Installation, Join, Balance, Receive, Send, History, Session, Restore. Get them as Markdown: on
the docs site, `react-native/llms-full.txt` holds every page in one file. In a checkout of the SDK
repository, read `docs/react-native/*.md`. Before you call a method, read its API page
(`api.md` links one page per method): the signature, whether it is sync or async, and the error
codes it throws.

## Rules

1. **Ask the user first** before: `npm install` (or the app's package manager), `pod install`,
   a native build, a commit, or any change to the app's existing wallet or key code.
2. **Seed words are money.** Never print, log, commit, paste into chat, or put into an error
   message the words from `exportMnemonic()`, or any file in the SDK's `dataDir`. Do not
   screenshot a screen that shows them. Use test words only.
3. **Test money only.** Use a signet federation (the guide names Fedi Testnet). Refuse
   `Network.Bitcoin` in a test build. Ask the user how to get test funds: the guide names no
   faucet.
4. **Install packages, not the monorepo.** Do not clone the SDK repo, run `pnpm`, `just` or Nix,
   build the Rust crate, or install `js/react-native/*` from a checkout. Those packages are
   private `0.0.0` builds with no native libraries.
5. **One version for both packages**, written exactly (the guide's `VERSION`). Never `latest`,
   `beta`, `^` or `~`. If the install fails with `ETARGET`, run
   `npm view @fedimint/react-native dist-tags` and tell the user. Do not pick another version
   by yourself.

## Pre-flight: run each, read the output

| Check              | Command                                                          | Pass                                                                  |
| ------------------ | ---------------------------------------------------------------- | --------------------------------------------------------------------- |
| React Native       | read `react-native` in `package.json`                            | 0.78 or newer                                                         |
| New Architecture   | `grep newArchEnabled android/gradle.properties`                  | `true`                                                                |
| Locale (CocoaPods) | `echo $LANG`                                                     | not empty, for example `en_US.UTF-8`                                  |
| Ruby               | `ruby -v` and `cat .ruby-version`                                | the same version (see Ruby below)                                     |
| Bundler            | `grep -A1 "BUNDLED WITH" Gemfile.lock` and `bundle -v`           | the same version                                                      |
| Java               | `echo $JAVA_HOME` and `"$JAVA_HOME/bin/java" -version`           | 17 or newer                                                           |
| Android SDK        | `echo $ANDROID_HOME`; `ls $ANDROID_HOME/ndk $ANDROID_HOME/cmake` | set; the NDK named by the app's `ndkVersion` is there; CMake is there |
| Package manager    | which lockfile is in the repo                                    | use that tool only                                                    |

**Ruby in an agent shell.** A non-interactive shell does not run `rbenv init`, so `ruby` is the
macOS system Ruby. Run `eval "$(rbenv init - zsh)"` (or `- bash` in bash; the name must match
the shell). If `rbenv install` says `definition not found`, run `brew upgrade ruby-build`.

## Decisions for an existing app

- **File path.** If the app already has a file library, use it. `dataDir` must be a plain path;
  strip `file://`. Install `react-native-fs` only if the app has none.
- **Bare or Expo.** Expo: the config plugin and a development build (Installation page). Bare: no
  plugin.
- **ABIs.** If `android/gradle.properties` already has `reactNativeArchitectures`, change that
  line; do not add a second one.
- **Pods.** If there is a `Gemfile`, run `bundle exec pod install`, else `pod install`. Add
  `--repo-update` the first time: one pod comes from the CocoaPods trunk by version.
- **Session.** Open the SDK once, in one module the app owns (Session page). Mount the first
  test screen inside the app's existing navigator, behind a debug or developer menu.
- **Restore.** Offer restore before the first open. The first open of an empty folder creates
  a seed, and the folder then refuses other words (`SeedMismatch`).
- **Saved invites.** Do not save the invite for launch. On launch, reopen the federation from
  `sdk.storedFederations()` and `sdk.reopenFederation(id)` (`federationOnLaunch`). This also
  finds a federation that `recover` restored. The invite is only needed to join.

## Code rules that agents get wrong

- Errors: an SDK call throws a wrapper, not an `Exception`. Never use `instanceof Exception`. Use
  `Exception.hasInner(err)`, then `Exception.getInner(err).code()` and compare with `ErrorCode`.
- Amounts are `bigint` millisatoshis. Sats to msat: `BigInt(sats) * 1000n`. Never
  `JSON.stringify` an amount, and never use `Number`.
- States with data are tagged unions: compare `state.tag` with `*_Tags.X`.
- A failed payment is a final state, not a thrown error. Read the final state.

## Failure table

| You see                                                                                           | Cause                                                        | Fix                                                    |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------ |
| `None of your spec sources contain a spec satisfying the dependency: uniffi-bindgen-react-native` | old CocoaPods trunk cache                                    | `pod install --repo-update`                            |
| `TurboModuleRegistry ... 'ReactNativeBindings' could not be found`                                | the native build is older than the install                   | rebuild the app; do not only reload JS                 |
| `TurboModuleRegistry ... 'PlatformConstants' could not be found` on Android                       | the app loaded another app's Metro bundle (port 8081 in use) | run Metro on a free port and point the debug app at it |
| `Invalid hook call`                                                                               | two copies of React                                          | `npm ls react`; dedupe                                 |
| `StorageInUse` after a Metro reload                                                               | the old JS still holds the folder                            | stop the app and start it again                        |
| `StorageInUse` when opening again in the same run                                                 | objects from the first session are still alive               | open once per run; offer restore before the first open |
| `SeedMismatch` on restore                                                                         | the folder already has a different seed                      | restore into an empty folder                           |
| `AlreadyJoined` on `recover`                                                                      | that federation is already in the folder                     | use `resumeRecovery(id)` (Restore page, step 04)       |
| Android build asks for `armeabi-v7a` or `x86`                                                     | `reactNativeArchitectures` not set                           | set it to `arm64-v8a,x86_64`                           |
| `npm install` is slow                                                                             | the bindings are several hundred MB                          | wait                                                   |

## Definition of done

Show the user each result:

1. `npm ls @fedimint/react-native @fedimint/react-native-bindings` shows the same exact version.
2. `grep -E "ReactNativeBindings|uniffi-bindgen-react-native" ios/Podfile.lock` finds both.
3. `grep reactNativeArchitectures android/gradle.properties` shows `arm64-v8a,x86_64`.
4. The app's typecheck passes (for example `npx tsc --noEmit`) for the new files.
5. On a simulator and an emulator, the Installation page's debug screen answers (it reports a
   12-word seed; show the count, not the words).
6. Join Fedi Testnet, and the balance reads `0` without an error.
7. For each later page you used, its "You will see" check passed on a device.

## Rollback

Remove both packages with the app's package manager, run `pod install`, and remove the
`reactNativeArchitectures` line (or restore the old one) and, for Expo, the plugin entry. The
SDK's wallet folder (`dataDir`) stays on the device until the app is deleted.
