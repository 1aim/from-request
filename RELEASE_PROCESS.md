# What to do to publish a new release

The release process relies on `cargo-release`, which must be installed using
`cargo install cargo-release` beforehand.

If anything inside `derive` was changed:

1. `cd derive; cargo release patch; cd ..`

Then continue publishing the actual `hyperdrive` crate:

1. Ensure all notable changes are in the changelog under "Unreleased".

2. Execute `cargo release <level>` to bump version(s), tag and publish
   everything.

   `<level>` can be one of `major|minor|patch`. If this is the first release
   (`0.1.0`), use `minor`, since the version starts out as `0.0.0`.

3. Go to the GitHub releases, edit the just-pushed tag. Copy the release notes
   from the changelog.
