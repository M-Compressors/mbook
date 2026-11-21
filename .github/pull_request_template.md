## Summary

<!-- What does this change and why? Link the issue it addresses (Fixes #...). -->

## Design documents

<!-- For a change to the file format or to the public API: which RFC or
     decision record under `rfcs/` was updated. Otherwise write "N/A". -->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- [ ] the same clippy run with `-p mbook --no-default-features --features read`, and with `--features write`
- [ ] `cargo test --workspace --all-features --locked`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked`
- [ ] New and changed functions are covered by tests (`cargo llvm-cov`); bug fixes include a regression test
- [ ] RFCs and decision records updated, and `mdbook build rfcs` passes, if behaviour or the format changed
- [ ] The status table in `README.md` is still true
- [ ] `CHANGELOG.md` updated for user-visible changes
- [ ] No new `#[allow(..)]`; any `#[expect(..)]` has a reason
