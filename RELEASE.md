# Releasing p2panda

_This example assumes we are publising version `1.2.0`._

\_Requires `cargo-release` to be installed (`cargo install cargo-release`)

## Checks and preparations

1. Check that the CI has passed on the p2panda project's
   [Github page](https://github.com/p2panda/p2panda).
2. Make sure you are on the `main` branch.
3. Run the test suite and make sure all tests pass:
   - `cargo test --all-features`
4. Make sure that all examples in each crate `README.md` (including the ones in the
   sub-folders) are still up-to-date with the latest API changes.

## Changelog time!

5. Check the git history for any commits on main that have not been mentioned
   in the _Unreleased_ section of `CHANGELOG.md` but should be.
6. Add an entry in `CHANGELOG.md` for this new release and move over all the
   _Unreleased_ stuff. Follow the formatting given by previous entries.
7. Remember to update the links to your release and the unreleased git log at
   the bottom of `CHANGELOG.md`.

## Release using [`cargo-release`](https://github.com/crate-ci/cargo-release)

8. If a new crate was introduced make sure to add the following to it's `Cargo.toml`.

```toml
[package.metadata.release]
release = true
publish = true
```

9. Commit any changes made so far during release, eg. `git add .` & `gc -m "Prepare for release"`.
10. Run the `cargo-release` in dry-run mode. This command performs tagged git releases for all
   crates and publish them to crates.io: `cargo-release 1.2.0`. Check the output, make sure
   everything looks correct and no errors.
11. Run the `cargo-release` for real:
    `cargo-release 1.2.0 --execute`.