# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/caretak3r/bdw/compare/v0.1.0...v0.1.1) - 2026-09-11

### Other

- *(readme)* regenerate demo screenshots with real terminal colors

## [0.1.0](https://github.com/caretak3r/bdw/releases/tag/v0.1.0) - 2026-09-11

### Added

- *(ui)* add theme system and markdown-rendered detail overlay
- detail overlay, epic grouping, search, actor filter, render tests
- app core + TUI shell — watcher, reducer, board/feed/header
- data layer — bd client, models, audit tailer, snapshot differ

### Fixed

- actor short name prefers username over noreply numeric id
- kill timed-out bd child on drop; ignore session artifacts

### Other

- enable git_only mode for release-plz (no crates.io publish)
- add README, CI/release workflows, and demo screenshots
- add MIT license and crate metadata
- record phase reviews and epic closure in beads audit log
- scaffold bdw — spec, TUI mock, real bd fixtures, beads tracking
- bd init: initialize beads issue tracking
