---
# https://vitepress.dev/reference/default-theme-home-page
layout: home

description: Multi-platform Fedimint SDK powered by Rust, WebAssembly, and UniFFI
title: Fedimint Sdk
titleTemplate: false

hero:
  name: Fedimint Sdk
  text: Building Ecash into Apps
  tagline: Robust, privacy-focused, WebAssembly and Native Mobile FFI powered
  actions:
    - theme: brand
      text: Get Started
      link: /core/getting-started
    - theme: alt
      text: Learn about Fedimint
      link: https://fedimint.org
    - theme: alt
      text: View on GitHub
      link: https://github.com/fedimint/fedimint-sdk

  image:
    src: /icon.png
    alt: Fedimint Logo

features:
  - icon: 🚀
    title: Multi-Platform Rust Client
    details: Exposes the robust fedimint-client via WebAssembly for web browsers and native UniFFI FFI bindings for iOS and Android.
  - icon: 💰
    title: Ecash Payments
    details: First-class support for joining federations, sending/receiving ecash, and managing token balances.
  - icon: ⚡
    title: Lightning Payments
    details: Ships with zero-setup Lightning Network payments via federation Lightning gateways.
  - icon: 🛠️
    title: State Management & Persistence
    details: Handles asynchronous state management, OPFS database persistence in browsers, and mobile filesystem storage.
  - icon: 🤫
    title: Privacy by Default
    details: Chaumian blinded tokens guarantee sender and receiver financial privacy.
  - icon: ⚙️
    title: Framework Agnostic
    details: Designed for vanilla JS, React, Next.js, Vite, React Native, and Expo applications.
---
