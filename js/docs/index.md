---
# https://vitepress.dev/reference/default-theme-home-page
layout: home

description: Robust, privacy-focused, powered by Rust
title: Fedimint Sdk
titleTemplate: false

hero:
  name: Fedimint Sdk
  text: Building Ecash into Apps
  tagline: Robust, privacy-focused, and WebAssembly-powered
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
    title: Rust-powered Client
    details: Exposes the robust, fault-tolerant fedimint-client (built in Rust) via WebAssembly/native modules. Lazy loading included for performance.
  - icon: 💰
    title: Ecash Payments
    details: Includes support for joining federations, sending/receiving ecash, and managing balances.
  - icon: ⚡
    title: Lightning Payments
    details: Ships with zero-setup Lightning Network payments.
  - icon: 🛠️
    title: State Management
    details: Handles the complex state management and storage challenges for browser & mobile wallets.
  - icon: 🤫
    title: Privacy Included
    details: Offers a privacy-centric wallet by default.
  - icon: ⚙️
    title: Framework Agnostic
    details: Designed as a "core" library compatible with vanilla JavaScript, laying the groundwork for future framework-specific packages.
---
