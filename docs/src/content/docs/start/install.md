---
title: Install
description: Install primum with Homebrew, the install script, or cargo.
---

## Homebrew

```bash
brew install erhangundogan/tap/primum
```

## Install script

Downloads the release for your platform and verifies its checksum:

```bash
curl -fsSL https://raw.githubusercontent.com/erhangundogan/primum/main/install.sh | sh
```

## Cargo

With a Rust toolchain:

```bash
cargo install primum --locked
```

## Platforms

Release binaries are built for macOS (Apple silicon and Intel) and Linux
(x86_64 and arm64).

## Optional tools

- **git 2.28 or later** for [history and sync](/guides/history-and-sync/).
  Without git, everything else works.
- **[`gh`](https://cli.github.com)**, logged in with `gh auth login`, for
  [GitHub Issues](/guides/github-issues/). primum never stores a token.

Check the install:

```bash
primum --version
```
