# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4](https://github.com/ryanclark/statusline/compare/v0.1.3...v0.1.4) - 2026-05-29

### Added

- add account and credits segments with Linux build support
- add statusline profiles subcommand
- resolve org, browser, profile, segments from active Claude account
- thread profile through fetch_usage
- parameterize browser session-key loading with profile
- add skip_update_check setting

### Fixed

- point update message at the new tap
- tolerate null extra_usage fields in API response

### Other

- add support for merge queue
- use GitHub App token for tap publishing
- use GitHub App token for release-plz
- satisfy rustfmt and clippy on stable 1.96
- update install for renamed homebrew-tap
- build and publish macOS and Linux (arm64/amd64) binaries via homebrew-tap
- update README for per-account config and profiles subcommand
- extract shared accounts module with browser/profile/segments fields
- spec for per-account browser profile & segment overrides
- Show errors if fetch usage fails

## [0.1.3](https://github.com/ryanclark/statusline/compare/v0.1.2...v0.1.3) - 2026-04-02

### Other

- Add segments with config, support for Brave + Firefox
- *(README)* remove duplicate note wording
- *(README)* formatting/text updates
- *(README)* add example image

## [0.1.2](https://github.com/ryanclark/statusline/compare/v0.1.1...v0.1.2) - 2026-03-30

### Other

- *(ci)* use checkout v5
- *(ci)* try better caching
- *(release-plz)* add SHA verification back

## [0.1.1](https://github.com/ryanclark/statusline/compare/v0.1.0...v0.1.1) - 2026-03-30

### Other

- *(ci)* use cache v5
- *(release)* use macOS latest
- *(ci)* sort out the cache
- *(release-plz)* set release_always to false
- *(release-plz)* use macOS runner, remove --no-verify
- *(release-plz)* add --no-verify to cargo publish
- *(release-plz)* use custom PAT for release PR creation
