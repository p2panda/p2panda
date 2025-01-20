# Releasing p2panda

_This example assumes we are publishing version `1.2.0`._

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
8. Commit any changes made so far during release with `git add .` and
   `git commit -m "Prepare for release v1.2.0"`.

## Publishing

Crates _must_ be published in the following order to account for
intra-workspace dependencies:

- `p2panda-core`
- `p2panda-discovery`
- `p2panda-store`
- `p2panda-sync`
- `p2panda-stream`
- `p2panda-net`
- `p2panda-blobs`

9. Move into the directory of the crate you wish to publish, taking into
    account the order listed above.
10. Open the manifest (`Cargo.toml`) and update the version at the top to
    `0.2.0`.
11. Update the version in the manifest of every `p2panda-` crate which depends
    on the one you are about to publish. For example, if you're publishing
    `p2panda-core` `0.2.0`, you need to update the manifest dependencies of every
    `p2panda-` crate to `p2panda-core = { path = "../p2panda-core", version = "0.2.0" }`.
12. Run `cargo publish --dry-run`. Check the output; ensure everything looks
    correct and there are no errors.
13. Run `cargo login` to ensure you're prepared to publish to `crates.io`.
14. Run `cargo publish` to publish.
15. Move on to the next crate you wish to publish, taking into account the
    order listed above.

## Tagging and release

16. Run `git tag v0.2.0` and push including your tags using `git push origin
    main --tags`.
17. Manually create a release on github, copying the changelog entry you authored
    into Github's [new release page](https://github.com/p2panda/p2panda/releases/new)'s 
    description field. Title it with your version `v0.2.0`.

## Troubleshooting

If you see a `failed to select a version` error (as below), see point `11`
above.

```
cargo publish -p p2panda-core --dry-run
    Updating crates.io index
error: failed to select a version for the requirement `p2panda-core = "^0.1.0"`
candidate versions found which didn't match: 0.2.0
location searched: /home/glyph/Projects/p2panda/p2panda/p2panda-core
required by package `p2panda-net v0.1.0 (/home/glyph/Projects/p2panda/p2panda/p2panda-net)`
```
