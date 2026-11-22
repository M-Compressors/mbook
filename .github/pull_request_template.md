## Summary

<!-- What does this change and why? Link the issue it addresses (Fixes #...). -->

## Documentation

<!-- For a change to the file format: which section of the RFC or which
     decision record under `docs/src/rfcs/` was updated. For a change to a
     crate: which page under `docs/src/<crate>/`, and its README if an
     example or the summary changed. Otherwise write "N/A". -->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] the same clippy run with `-p mbook --no-default-features --features read`, and with `--features write`
- [ ] `cargo test --workspace --all-features --locked`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked`
- [ ] New and changed functions are covered by tests (`cargo llvm-cov`); bug fixes include a regression test
- [ ] The RFC and decision records updated if the format changed; the pages and the README of the crate updated if its behaviour changed; `mdbook build docs` passes
- [ ] The status table in `README.md` is still true
- [ ] `CHANGELOG.md` updated for user-visible changes
- [ ] No new `#[allow(..)]`; any `#[expect(..)]` has a reason
