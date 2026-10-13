---
title: "Release v0.1.0"
status: done
impact: 8
urgency: 6
risk: 4
effort: 3
needs: []
finished: 2026-10-13
source: "primum"
---

## Steps

Each needs the owner's approval when it happens.

1. Push `main` and watch CI.
2. Add the `TAP_TOKEN` secret to the repository.
3. Tag `v0.1.0` and watch the release workflow (draft, build, smoke, publish, tap).
4. `cargo publish --dry-run`, then `cargo publish`.

---

## 2026-10-13

Released 2026-10-13: GitHub release v0.1.0 (four targets, all smoke-tested), Homebrew formula in erhangundogan/homebrew-tap, and crates.io.
