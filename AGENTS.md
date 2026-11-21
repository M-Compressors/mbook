# Repository Guidelines

These instructions apply to all repository-owned code and documents.

## Design documents come first

- The file format is specified by `rfcs/src/0001-mbook-format.md` and the API
  of `mbook` by `rfcs/src/0002-mbook-api.md`. Read them, and the
  decision records in `rfcs/src/adr/`, before changing behaviour.
- A change to the format or to a public API updates its RFC in the same
  change. Code that disagrees with an RFC is an incomplete change.
- A choice between alternatives is recorded: as a decision record
  (`rfcs/src/adr/NNNN-title.md`, listed in `adr/overview.md` and
  `SUMMARY.md`) for the format, and in the "Rationale and alternatives" table
  of the RFC for the library.
- A detail the format author has not confirmed is marked *proposed* in its
  record. Ask before building on one.
- Documents are written in English. Normative text uses the key words of
  RFC 2119 in capitals, and only inside "Reference-level explanation".
- Describe what exists. Mark unimplemented paths explicitly instead of
  documenting intended behaviour as if it were there; the status table in
  `README.md` is kept current.
- Build the book with `mdbook build rfcs` after editing it.

## Architecture and API boundaries

- `mbook` has two central types: `BookReader` reads lazily, `Book` is
  the in-memory model. Editing goes through `Book` only; there is no
  streaming editor and none is planned.
- Keep the public API small and difficult to misuse. Table lookups take
  `&self` and do no I/O; calls that touch a source start with `read_` and
  take `&mut self`.
- No type of a dependency (mopenzl, `markdown`, `uuid`) appears in a public
  signature. Their errors are wrapped as the `source()` of a crate error.
- Chapters and pictures are addressed by typed ids (`ChapterId`,
  `PictureId`) in `BookReader` and by path in `Book`. Ids have no public
  constructor.
- Every length and count read from a file is bounded by `Limits` before
  memory is allocated for it.
- Prefer borrowed inputs when ownership is unnecessary and standard traits
  (`From`, `Default`, `Display`, `IntoIterator`) over inherent methods.
- Document every public item, with `# Errors`, `# Panics` and `# Examples`
  sections where they apply. Examples are doc tests.
- Shared dependencies and package metadata live in the workspace
  `Cargo.toml` (`[workspace.dependencies]`, `[workspace.package]`); crates
  refer to them with `workspace = true`.
- Use an established crate for a mechanism; write only what is a rule of the
  mbook format. Byte cursors, encoders and decoders of standard encodings,
  temporary files, identifiers and parsers of other formats come from
  crates.io: `bytes`, `percent-encoding`, `tempfile`, `uuid`, `markdown`,
  `github-slugger`. Before writing a helper, look for a crate, and read its
  source for what it actually does. Keep own code only when no crate meets
  the rule of the RFC, the minimum Rust version and the license, and record
  the crates that were ruled out in RFC 0002, as is done for the AVIF size.
- Every crate carries the licenses as symbolic links to the files at the
  repository root, so a published package contains them without a second
  copy to keep in sync:
  `ln -s ../../LICENSE-MIT LICENSE-MIT` and
  `ln -s ../../LICENSE-APACHE LICENSE-APACHE` in the crate's folder.
- Feature-gated items must keep every single-feature build warning-free,
  including its documentation: a link to an item of a disabled feature needs
  a fallback definition (see the top of `src/lib.rs`).

## Errors

- Use `thiserror` for every error type. Fallible library code returns
  `Result`; production code does not use `unwrap`, `expect` or `panic!` for
  recoverable failures. `todo!` does not appear in committed code.
- Public error enums are `#[non_exhaustive]`. Lower-level errors are joined
  to the public `Error` with typed variants and `#[from]` or `#[source]`, so
  the source chain is kept.
- A message either includes its source or exposes it through `source()`,
  not both.
- Rules about one argument are checked by the call that receives it; rules
  that connect several parts of a book are checked by `Book::check` and
  reported together as `Problems`.
- Test messages that are part of the API contract, conversions and source
  chains.

## Lints

- Lint levels are set once, in `[workspace.lints]` of the workspace
  `Cargo.toml`: `unsafe_code` is forbidden, Clippy's `pedantic` group is
  denied, and so are `unwrap_used`, `expect_used`, `panic`, `dbg_macro`,
  `print_stdout` and `print_stderr`. `clippy.toml` relaxes the first three
  inside tests.
- Fix warnings rather than suppressing them. `#[allow]` is denied. For a
  demonstrated false positive use the narrowest
  `#[expect(lint, reason = "..")]`.

## Tests and coverage

- Every new or changed function is exercised by tests. Inspect a coverage
  report (`cargo llvm-cov`) rather than assuming a passing test reached the
  code; only closures that map errors which cannot occur may be uncovered.
- Unit tests sit beside private logic. Public behaviour goes into
  integration tests under `crates/mbook/tests/`, and public examples are
  doc tests.
- Give tests behaviour-oriented names, cover success, error, empty and
  boundary cases, and keep each test to one behaviour.
- Integration tests return `Result` and use `?`; helper functions there do
  not `unwrap`.
- Add a regression test before fixing a bug.
- `tests/format/` holds the end-to-end tests. `reading.rs` reads a
  checked-in file, `tests/fixtures/example.mbook`, and `writing.rs` asserts
  that the writer still produces exactly that file. After a deliberate change
  of the written bytes, rewrite it with
  `cargo test --test format -- --ignored regenerate` and say so in
  `CHANGELOG.md`. `raw.rs` builds containers by hand for files the writer
  never produces; test the reader's rules with those.

## Required completion checks

After changing Rust code:

1. `cargo fmt --all`, then `cargo fmt --all -- --check`.
2. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
   and the same with `-p mbook --no-default-features --features read`
   and `--features write`.
3. `cargo test --workspace --all-features --locked`, and with each feature
   alone.
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked`,
   and with each feature alone.
5. Check coverage for every changed function.
6. Update the RFCs, the status table in `README.md`, and `CHANGELOG.md`.

After changing a document under `rfcs/`: `mdbook build rfcs`, and check that
new links resolve.
