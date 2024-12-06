# Releasing p2panda

_This example assumes we are publising version `1.2.0`._

_Remember to update the version number accordingly!_

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

## Tagging and versioning

8. Bump the package version in `Cargo.toml` by hand.
9. Commit the version changes with a commit message `1.2.0`.
10. Run `git tag v1.2.0` and push (including your tags) using `git push origin
    main --tags`.

## Publishing releases

11. Copy the changelog entry you authored into Github's [new release
    page](https://github.com/p2panda/p2panda/releases/new)'s description field.
    Title it with your version `v1.2.0`.
12. Run `cargo publish` from the workspace root.
13. Do a dance of celebration!
