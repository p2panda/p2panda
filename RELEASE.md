# Releasing p2panda

> p2panda crates are organised in a single Rust workspace (mono-repository). Until our first stable,
> major release we maintain _one version_ per release _for all crates_. These versions will indicate
> the progress towards `v1.0.0`.

_This example assumes we are publishing version `v0.2.0`._

## Checks and preparations

1. Check that the CI has passed on the p2panda project's [GitHub page](https://github.com/p2panda/p2panda).
2. Make sure you are on the `main` branch.
3. Run the test suite and make sure all tests pass: `cargo test --all-features`
4. Make sure that all examples in each crate `README.md` (including the ones in the sub-folders) are
   still up-to-date with the latest API changes.

## Changelog time!

5. Check the git history for any commits on main that have not been mentioned in the _Unreleased_
   section of `CHANGELOG.md` but should be.
6. Add an entry in `CHANGELOG.md` for this new release and move over all the _Unreleased_ stuff.
   Follow the formatting given by previous entries.
7. Remember to update the links to your release and the unreleased git log at the bottom of
   `CHANGELOG.md`.
8. Commit any changes made so far during release with `git add .` and
   `git commit -m "Prepare for release v0.2.0"`.

## Publishing

9. Open the manifest (`Cargo.toml`) of each crate and update the version at the top to `0.2.0`.
10. Do a dry run via `cargo publish --workspace --dry-run`. Check the output; ensure everything
    looks correct and there are no errors.
11. Run `cargo login` to ensure you're prepared to publish to `crates.io`.
12. Finally, publish everything in the workspace with `cargo publish --workspace`

## Tagging and release

13. Run `git tag v0.2.0` and push including your tags using `git push origin main --tags`.
14. Manually create a release on GitHub, copying the changelog entry you authored into Github's [new
    release page](https://github.com/p2panda/p2panda/releases/new)'s description field. Title it
    with your version `v0.2.0`.
