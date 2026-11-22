# Contributing to mbook

Thank you for helping. Bug reports, feedback on the format, sample books that
break something, documentation fixes, and code are all welcome. By
participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
Security issues are reported privately; see [SECURITY.md](SECURITY.md).

## Before you start

- For anything beyond a small fix, open an issue first.
- Read the documentation in [`docs/`](docs/src/introduction.md). The format
  is specified by [RFC 0001](docs/src/rfcs/0001-mbook-format.md); each crate
  is described by its own pages:
  [`mbook`](docs/src/mbook/index.md),
  [`mbook-converter`](docs/src/mbook-converter/index.md) and
  [`mbook-ui`](docs/src/mbook-ui/index.md).
- **A change of behaviour starts in the documents.** A change to the file
  format updates RFC 0001, and a choice between alternatives gets a decision
  record under `docs/src/rfcs/adr/`. A change to a crate updates its pages,
  and its README when an example or the summary there is affected. Both
  happen in the same pull request as the code.
- The documents are written in English.

## Setting up

```sh
git clone https://github.com/M-Compressors/mbook.git
cargo test --workspace --all-features --locked
```

The minimum supported Rust version is 1.89. The documentation needs
[mdBook](https://github.com/rust-lang/mdBook) and
[mdbook-mermaid](https://github.com/badboy/mdbook-mermaid); see
[the introduction](docs/src/introduction.md#building-this-site).

## Code guidelines

The full rules are in [`AGENTS.md`](AGENTS.md). In short:

- public signatures use the crate's own types, never those of a dependency;
- errors are `thiserror` types, and library code does not panic, `unwrap` or
  print;
- lints are set once in the workspace `Cargo.toml`, and a lint is silenced
  only with a narrow `#[expect(.., reason = "..")]`, never with `#[allow]`;
- every public item is documented, and every new or changed function is
  exercised by a test.

## Before opening a pull request

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy -p mbook --all-targets --no-default-features --features read --locked -- -D warnings
cargo clippy -p mbook --all-targets --no-default-features --features write --locked -- -D warnings
cargo test --workspace --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
mdbook build docs
```

Update the documentation and `CHANGELOG.md`, and describe how you verified the change.
