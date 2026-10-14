# primum

Prioritized work items as plain markdown files, from the command line, for
any number of projects. Every ticket follows a lifecycle, the steps it goes
through from created to closed; an engine that runs lifecycles by
themselves is in progress. primum is changing fast; the documentation
follows the current release.

Documentation: <https://erhangundogan.github.io/primum/>

## Install

```bash
brew install erhangundogan/tap/primum
```

or with the install script, which verifies the download's checksum:

```bash
curl -fsSL https://raw.githubusercontent.com/erhangundogan/primum/main/install.sh | sh
```

or, with a Rust toolchain:

```bash
cargo install primum --locked
```

## License

Apache-2.0
