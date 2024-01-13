# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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


