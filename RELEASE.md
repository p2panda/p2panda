# Releasing p2panda

_This example assumes we are publishing version `1.2.0`._

_Requires `cargo-release` to be installed (`cargo install cargo-release`)_

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

## Publish using [`cargo-release`](https://github.com/crate-ci/cargo-release)

8. If a new crate was introduced make sure to add the following to it's `Cargo.toml`.

```toml
[package.metadata.release]
publish = true
```

9. Commit any changes made so far during release with `git add .` and
   `git commit -m "Prepare for release v1.2.0"`.
10. Run the `cargo-release` in dry-run mode `cargo-release release 1.2.0`. Check the output, make
    sure everything looks correct and no errors.
11. Run the `cargo-release` for real `cargo-release release 1.2.0 --execute`. This command
    publishes all crates to crates.io.

## Tagging and release

12. Run `git tag v1.2.0` and push including your tags using `git push origin
    main --tags`.
13. Manually create a release on github, copying the changelog entry you authored 
    into Github's [new release page](https://github.com/p2panda/p2panda/releases/new)'s 
    description field. Title it with your version `v1.2.0`.
