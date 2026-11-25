# Releasing p2panda

> p2panda crates are organised in a single Rust workspace (mono-repository). Until our first stable,
> major release we maintain _one version_ per release _for all crates_. These versions will indicate
> the progress towards `v1.0.0`.

Every release has its own branch with a strict naming scheme: `releases/vX.X.X`. It is created from
`main` and interactively rebased on top of the previous release tag to remove the commits we don't
want to release yet.

_This example assumes we are publishing version `v0.8.2`._

## 1. Prepare the release branch

1. Make sure you are on the latest `main`: `git switch main && git pull`.
2. Create the release branch from it: `git switch -c releases/v0.8.2`.
3. Rebase it on top of the previous release and drop every commit which should not be part of this
   release: `git rebase -i --no-update-refs v0.8.1`.

## 2. Checks

1. Push the release branch and check the CI has passed.
2. Run the test suite and make sure all tests pass: `cargo test --all-features`
3. Make sure that all examples in each crate `README.md` (including the ones in the sub-folders) are
   still up-to-date with the latest API changes.

## 3. Changelog time!

1. Add an entry in `CHANGELOG.md` for this new release. Move over everything from the _Unreleased_
   section which is part of this release, leave the rest in _Unreleased_. Follow the formatting
   given by previous entries.
2. Update the links at the bottom of `CHANGELOG.md`:

   ```
   [unreleased]: https://github.com/p2panda/p2panda/compare/v0.8.2...HEAD
   [0.8.2]: https://github.com/p2panda/p2panda/compare/v0.8.1...v0.8.2
   ```

## 4. Publishing

1. Update the version to `0.8.2` in the workspace `Cargo.toml` and in the manifest of each crate.
2. Do a dry run via `cargo publish --workspace --dry-run --allow-dirty`.
3. Commit the changes on the release branch: `git commit -am v0.8.2`.
4. Run `cargo login` to ensure you're prepared to publish to `crates.io`.
5. Publish everything in the workspace with `cargo publish --workspace` or use more fine-grained
   commands if you don't want to publish everything (`-p`, etc.).

## 5. Tagging and release

1. Tag the release commit: `git tag v0.8.2`.
2. Push the branch and the tag: `git push origin releases/v0.8.2 refs/tags/v0.8.2`.
3. Manually create a release on GitHub, copying the changelog entry you authored into GitHub's [new
   release page](https://github.com/p2panda/p2panda/releases/new)'s description field. Title it with
   your version `v0.8.2`.

## 6. Bring the release back to `main`

1. Switch to `main` and apply the release commit (changelog and version bumps) to it:
   `git switch main && git cherry-pick v0.8.2`.
2. Resolve the conflicts in `CHANGELOG.md`, the new release section must be there and its entries
   must be removed from _Unreleased_.
3. Push to `main` directly.
