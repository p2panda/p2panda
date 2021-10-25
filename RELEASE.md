# Releasing p2panda

We are always releasing `p2panda-rs` and `p2panda-js` together.

_This is an example for publising version `1.2.0`._

## Checks and preparations

1. Check that the CI has passed on the p2panda project's [Github page](https://github.com/p2panda/p2panda).
2. Make sure you are on the `main` branch.
3. Run the test suites and make sure all tests pass:
    - p2panda-rs: `cargo test`
    - p2panda-js: `npm run test` and also test `npm run build`

## Changelog time!

4. Check the git history for any commits on main that have not been mentioned in the _Unreleased_ section of `CHANGELOG.md` but should be.
5. Add an entry in `CHANGELOG.md` for this new release and move over all the _Unreleased_ stuff. Follow the formatting given by previous entries.

## Tagging and versioning

6. Bump the package version in `package.json` using `npm version --no-git-tag-version [major|minor|patch]` (this is using [semantic versioning](https://semver.org/)).
7. Bump the package version in `Cargo.toml` by hand.
8. Commit the version changes with a commit message `1.2.0`.
9. Run `git tag v1.2.0` and push including your tags using `pit push origin main --tags`.

## Publishing releases

10. Copy the changelog entry you authored into Github's [new release page](https://github.com/p2panda/p2panda/releases/new)'s description field. Title it with your version `v1.2.0`.
11. Run `cargo publish` in `p2panda-rs`.
12. Run `npm run build` in `p2panda-js`.
13. Run `npm pack --dry-run` to check the file listing you are about to publish doesn't contain any unwanted files.
14. Run `npm publish` and check the window for any birds outside your window.
