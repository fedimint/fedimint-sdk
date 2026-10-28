---
title: 'Welcome'
description: 'What the SDK does, what is proven, and where to start.'
---

# Add ecash and Lightning to the app you already have

Join a federation, hold ecash, send and receive it, and restore from seed words, with one typed SDK.
{.subtitle}

```sh no-check [terminal]
npm install --save-exact @fedimint/react-native@0.1.0-beta.2 @fedimint/react-native-bindings@0.1.0-beta.2
cd ios && pod install
```

```tsx no-check [App.tsx]
const session = await openSdk({ dataDir })
const fed = await session.sdk.join(InviteCode.parse(code))
const msat = useBalance(fed)
return <Text>{`${msat / 1000n} sats`}</Text>
```

## What is Fedimint {#what-is-fedimint}

[Fedimint](https://fedimint.org) is an open source protocol that lets a group of guardians, a _federation_, hold bitcoin together for a community. The federation issues **ecash**: digital notes, backed by that bitcoin, that your wallet keeps on the device. Ecash payments are fast, cheap and private, and a Lightning gateway connects them to the Lightning Network.

<figure class="flow-figure" aria-hidden="true">
  <svg
    viewBox="0 0 720 230"
    role="presentation"
    style="width: 100%; height: auto; font-family: var(--body)"
  >
    <defs>
      <marker
        id="arr"
        viewBox="0 0 10 10"
        refX="9"
        refY="5"
        markerWidth="7"
        markerHeight="7"
        orient="auto-start-reverse"
      >
        <path d="M0,0 L10,5 L0,10 z" style="fill: var(--grey-2)" />
      </marker>
    </defs>
    <rect
      x="10"
      y="80"
      width="180"
      height="76"
      rx="12"
      style="
        fill: var(--surface-blue);
        stroke: var(--blue);
        stroke-width: 1.5;
      "
    />
    <text
      x="100"
      y="110"
      text-anchor="middle"
      style="fill: var(--ink); font-weight: 600; font-size: 15px"
    >
      Your app + SDK
    </text>
    <text
      x="100"
      y="132"
      text-anchor="middle"
      style="fill: var(--grey-1); font-size: 12px"
    >
      holds ecash notes
    </text>
    <rect
      x="270"
      y="20"
      width="200"
      height="190"
      rx="12"
      style="fill: var(--surface); stroke: var(--line); stroke-width: 1.5"
    />
    <text
      x="370"
      y="46"
      text-anchor="middle"
      style="fill: var(--ink); font-weight: 600; font-size: 15px"
    >
      Federation
    </text>
    <g style="fill: var(--paper); stroke: var(--grey-3)">
      <rect x="290" y="62" width="76" height="34" rx="8" />
      <rect x="374" y="62" width="76" height="34" rx="8" />
      <rect x="290" y="104" width="76" height="34" rx="8" />
      <rect x="374" y="104" width="76" height="34" rx="8" />
    </g>
    <g text-anchor="middle" style="fill: var(--grey-1); font-size: 12px">
      <text x="328" y="84">guardian</text>
      <text x="412" y="84">guardian</text>
      <text x="328" y="126">guardian</text>
      <text x="412" y="126">guardian</text>
    </g>
    <text
      x="370"
      y="166"
      text-anchor="middle"
      style="fill: var(--grey-1); font-size: 12px"
    >
      3 of 4 must agree
    </text>
    <text
      x="370"
      y="186"
      text-anchor="middle"
      style="fill: var(--grey-1); font-size: 12px"
    >
      holds the bitcoin
    </text>
    <rect
      x="540"
      y="40"
      width="170"
      height="56"
      rx="12"
      style="fill: var(--surface); stroke: var(--line); stroke-width: 1.5"
    />
    <text
      x="625"
      y="73"
      text-anchor="middle"
      style="fill: var(--ink); font-weight: 600; font-size: 14px"
    >
      Lightning gateway
    </text>
    <rect
      x="540"
      y="140"
      width="170"
      height="56"
      rx="12"
      style="fill: var(--surface); stroke: var(--line); stroke-width: 1.5"
    />
    <text
      x="625"
      y="173"
      text-anchor="middle"
      style="fill: var(--ink); font-weight: 600; font-size: 14px"
    >
      Lightning Network
    </text>
    <g style="stroke: var(--grey-2); stroke-width: 1.5; fill: none">
      <path
        d="M190,104 L270,104"
        marker-end="url(#arr)"
        marker-start="url(#arr)"
      />
      <path
        d="M470,68 L540,68"
        marker-end="url(#arr)"
        marker-start="url(#arr)"
      />
      <path
        d="M625,96 L625,140"
        marker-end="url(#arr)"
        marker-start="url(#arr)"
      />
      <path
        d="M100,156 L100,200"
        marker-end="url(#arr)"
        stroke-dasharray="4 3"
      />
    </g>
    <g
      style="
        fill: var(--grey-1);
        font-size: 11px;
        font-family: var(--mono);
      "
    >
      <text x="230" y="96" text-anchor="middle">ecash</text>
      <text x="505" y="60" text-anchor="middle">ecash</text>
      <text x="635" y="122">sats</text>
      <text x="110" y="186">notes to another wallet</text>
    </g>
  </svg>
</figure>

In short: your app holds ecash and talks to one federation. The federation reaches Lightning through a gateway. Your app can also give notes straight to another wallet.

## What this SDK does {#what-this-sdk-does}

This guide follows one path, from an empty project to a wallet that joins a federation, holds ecash, moves it two ways, lists its history and restores itself from seed words. It uses two npm packages. You import everything from `@fedimint/react-native`. It adds `openSdk` and two React hooks, `useBalance` and `useOperation`. `@fedimint/react-native-bindings` holds the native part, generated from the Rust crate `rust/fedimint-sdk`. Your app lists both.

The packages ship prebuilt native libraries for iOS 15.0 or newer and Android API 24 or newer. Nothing compiles when you install them. They need the New Architecture, and they do not run in Expo Go. Expo apps use a config plugin and a development build.

Amounts are millisatoshis (msat), as a `bigint`. A failed payment is a state that you observe, not an error that is thrown. Every code block in this guide is compiled against the packages before it is published here.

This guide does not cover on chain send and receive. A federation that supports them gives you `fed.onchain()`.

## Where to start {#where-to-start}

- [**Quickstart**](./quickstart.md): From an empty app to a balance on screen in one page.
- [**Guides**](./join.md): One task each: join, balance, receive, send, history, share the session, restore.
- [**Reference**](./errors.md): Errors, amounts, and seed words.

## For AI agents {#for-ai-agents}

An agent can follow this guide from start to end. Every page is also plain Markdown. Give the agent one of these files:

- [**llms-full.txt**](/react-native/llms-full.txt): Every page in one file. Use this one first.
- [**llms.txt**](/react-native/llms.txt): The list of pages, with a link to each page as Markdown.
- <a href="/react-native/md/quickstart.md" target="_blank"><strong>md/quickstart.md</strong></a>: One page as plain Markdown. Every page has one, at `md/<page>.md`.
- [**SKILL.md**](https://github.com/fedimint/fedimint-sdk/blob/main/skills/fedimint-react-native-install/SKILL.md): An agent skill for a first install in an existing app: checks, safety rules, failures and a definition of done.

Then give the agent this prompt:

```text no-check
Read llms-full.txt from the Fedimint SDK for React Native guide.
Add the SDK to this React Native app. Follow the pages in order,
from Installation to Restore from seed words. Install the exact
package versions that the guide gives. Make sure that each step
works before you start the next one.
```

## The pages {#the-pages}

- [Quickstart](./quickstart.md)
- [Installation](./installation.md)
- [Join a federation](./join.md)
- [Show a balance](./balance.md)
- [Receive money](./receive.md)
- [Send money](./send.md)
- [See the history](./history.md)
- [Share the session](./session.md)
- [Restore from seed words](./restore.md)
- [Errors](./errors.md)
- [Amounts](./amounts.md)
- [Seed words](./seed-words.md)
