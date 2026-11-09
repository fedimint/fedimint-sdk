# Changelog

Release notes for the Web SDK, `@fedimint/web-sdk` on npm, newest first. Each section also
becomes that version's GitHub Release.

The version bump pull request writes the next section from the commits since the last
release, grouped under Breaking, Added, Changed and Fixed, and the section can be edited
in that pull request. The release refuses a version that has no section. See "Release
notes" in [RELEASING.md](../../../RELEASING.md#release-notes).

## 0.1.0

### Added

- implement Nostr relay client & SDK federation discovery (#471)
- add a multisig facade that an extension backs (#508)
- let another crate add client modules to an instance (#507)

### Changed

- Built against fedimint 0.13.0, where 0.1.0-beta.4 was built against fedimint 0.13.0-beta.1.

### Fixed

- report a v1 lightning send's success with fedimint 0.13.0 (#529)
- record how much a multisig sweep moved once it is done (#520)

## 0.1.0-beta.4

No changes for users since 0.1.0-beta.3.

## 0.1.0-beta.3

The first release of the Web SDK.
