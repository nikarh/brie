# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.13] - 2025-12-16
### Details
#### Added
- Support newer nvcuda versions by @nikarh

#### Changed
- Bump peter-evans/create-pull-request from 6 to 7 (#32) by @dependabot[bot] in #32
- Bump actions/create-github-app-token from 1 to 2 (#33) by @dependabot[bot] in #33
- Updated deps, added cargo-deny by @nikarh
- Clippy by @nikarh
- Bump actions/checkout from 4 to 5 (#34) by @dependabot[bot] in #34
- Bump actions/upload-artifact from 4 to 5 (#35) by @dependabot[bot] in #35
- Bump actions/checkout from 5 to 6 (#36) by @dependabot[bot] in #36
- Bump actions/upload-artifact from 5 to 6 (#38) by @dependabot[bot] in #38
- Bump peter-evans/create-pull-request from 7 to 8 (#37) by @dependabot[bot] in #37
- Release v0.0.12 (#39) by @nikarh-release-bot[bot] in #39

#### Fixed
- Fixed build by @nikarh
- Clippy by @nikarh
- Failing test by @nikarh


## [0.0.12] - 2025-12-16
### Details
#### Added
- Support newer nvcuda versions by @nikarh

#### Changed
- Bump peter-evans/create-pull-request from 6 to 7 (#32) by @dependabot[bot] in #32
- Bump actions/create-github-app-token from 1 to 2 (#33) by @dependabot[bot] in #33
- Updated deps, added cargo-deny by @nikarh
- Clippy by @nikarh
- Bump actions/checkout from 4 to 5 (#34) by @dependabot[bot] in #34
- Bump actions/upload-artifact from 4 to 5 (#35) by @dependabot[bot] in #35
- Bump actions/checkout from 5 to 6 (#36) by @dependabot[bot] in #36
- Bump actions/upload-artifact from 5 to 6 (#38) by @dependabot[bot] in #38
- Bump peter-evans/create-pull-request from 7 to 8 (#37) by @dependabot[bot] in #37

#### Fixed
- Fixed build by @nikarh
- Clippy by @nikarh
- Failing test by @nikarh


## [0.0.11] - 2024-05-14
### Details
#### Added
- Added wine-tkg support (#28) by @nikarh in #28

#### Changed
- Bump peter-evans/create-pull-request from 5 to 6 (#26) by @dependabot[bot] in #26
- Bump mathieudutour/github-tag-action from 6.1 to 6.2 (#27) by @dependabot[bot] in #27


## [0.0.10] - 2024-01-14
### Details
#### Fixed
- Make all sunshine fields optional in deserialization (#24)


## [0.0.9] - 2024-01-13
### Details
#### Fixed
- Batch override dlls (#21) (#22)


## [0.0.8] - 2024-01-13
### Details
#### Changed
- Append to WINEDLLOVERRIDES instead of overriding (#19)


## [0.0.7] - 2024-01-09
### Details
#### Fixed
- Fixed native unit cd and debug log (#17)


## [0.0.6] - 2024-01-09
### Details
#### Changed
- Set logo and icon in steam shortcut (#15)

#### Fixed
- Native units accept relative commands when cd is defined (#14)


## [0.0.5] - 2024-01-08
### Details
#### Changed
- Replaced intotify crate with a cross-platform notify (#11)

#### Fixed
- Set current dir for native units (#12)


## [0.0.4] - 2024-01-04
### Details
#### Changed
- Replaced rustls with openssl and lazy_static with OnceLock (#9)


## [0.0.3] - 2024-01-02
### Details
#### Fixed
- Header in release message (#6)
- Deserialization of unit enum (#7)


## [0.0.2] - 2023-12-30
### Details
#### Added
- Added empty CHANGELOG.md for the CI (#4)

#### Changed
- Preparation is working
- Better error context
- Working runner
- Added nvidia-libs
- Added watch command
- Added native unit support
- Use workspace for common package fields
- Prepared for public release and added CI
- Bumped checkout action to @4 (#2)

#### Fixed
- Filter closed prs by label for release (#3)


