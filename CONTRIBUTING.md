# Contributing to primum

## Build and test

```bash
make build-dev    # target/debug/primum
make test         # the full suite, as CI runs it
make fmt          # format; use this, not bare `cargo fmt`
make fmt-check    # what CI runs
make lint-check   # clippy with -D warnings, as CI runs it
make e2e-install  # once: Playwright and Chromium
make e2e          # the board in a real browser
```

The Rust version is pinned in `rust-toolchain.toml`; change it nowhere else.

## Pull requests

- Open a PR against `main`; checks must be green.
- One change per PR. Put the reasoning in commit messages and keep the PR
  description short.
- A behaviour change comes with a test that failed before it.
- Stage files by name, never `git add -A`. Enable the commit guard once per
  clone with `make hooks`.
- Test fixtures use non-ASCII names (`Özgür`, `Işıl`, `Gökçe`).

## License

By contributing you agree that your contributions are licensed under the
Apache-2.0 license.
