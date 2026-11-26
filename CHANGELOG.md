# Changelog

All notable changes to flupmix are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The release workflow publishes the
section matching the pushed tag as the GitHub release notes.

## [Unreleased]

## [0.5.3] - 2026-11-26

### Changed
- GitHub releases are titled with the version only (`v0.5.3`).

## [0.5.2] - 2026-11-26

### Fixed
- `install.sh` also installs PNG icons and force-refreshes the per-user icon cache, so launchers such as Vicinae show the flupmix icon.

## [0.5.1] - 2026-11-26

### Changed
- GitHub Actions split into dependent jobs with a shared setup action; release packages the CI-built binary instead of rebuilding.
- AUR PKGBUILDs for `flupmix` and `flupmix-bin` in `packaging/aur`.

## [0.5.0] - 2026-11-26

### Added
- Meter style setting: continuous bar or segmented LED ladder.

## [0.4.1] - 2026-11-26

### Changed
- Redesigned README: animated hero, real screenshots, signal-flow diagram and section headers.

## [0.4.0] - 2026-11-26

### Added
- Settings dialog (live apply, stored in `~/.config/flupmix/settings.toml`): close to tray, start minimised, launch at login, app routing on/off, meter refresh/hold/fall-off/clip level, fader wheel step/reset level/snap to 0 dB, colour palette (Cyberdream high contrast, Cyberdream, Mint), remember window size, keyboard shortcuts (Alt+1..9 mute strip, Ctrl+Shift+1..9 load preset).

## [0.3.1] - 2026-11-26

### Fixed
- Release builds now reuse the CI Rust cache (shared cache key, saved only from `main`).

## [0.3.0] - 2026-11-26

### Added
- Presets: save, load and delete named snapshots of the whole mixer (strips, levels, routing, devices, app rules) from the new Presets dialog. Stored in `~/.config/flupmix/presets/`.

### Fixed
- AppImage now ships a valid PNG `.DirIcon`.

## [0.2.0] - 2026-11-26

### Added
- Per-input send level sliders for the hardware bus and every virtual bus.
- AppImage release asset (bundles the binary; PipeWire, Wayland and GPU libraries come from the host).

### Changed
- High-contrast cyberdream colour theme.
- Input strips keep a fixed width.
- Buses section keeps its natural width and scrolls on its own instead of being clipped.

## [0.1.0] - 2026-11-26

First release.

### Added
- Mixer built on PipeWire (no JACK): a single `pw_filter` node mixes up to 16 stereo input strips into a hardware bus and up to 8 virtual buses.
- Input strips as virtual sinks or hardware capture sources, with fader (-60 to +12 dB), mute, global solo, post-fader peak meters with hold and clip indicator, and per-bus sends.
- Hardware bus that follows the system default sink or any chosen sink; virtual buses exposed to other apps as virtual microphones.
- Application routing: a catch-all strip plus per-strip assigned and blocked lists, matched by process binary or application name, with a running-apps popover.
- Patchbay tab: qpwgraph-style graph view with drag-to-link, unlink, search and scoped view.
- GPUI interface in the flup·trade palette with bundled IBM Plex Sans and Lilex fonts.
- TOML persistence in `~/.config/flupmix/config.toml`, system tray (StatusNotifierItem) with keep-running on window close.
- Desktop entry, icon and `install.sh` for per-user installation.

[Unreleased]: https://github.com/Niller2005/flupmix/compare/v0.5.3...HEAD
[0.5.3]: https://github.com/Niller2005/flupmix/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/Niller2005/flupmix/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/Niller2005/flupmix/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/Niller2005/flupmix/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/Niller2005/flupmix/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Niller2005/flupmix/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/Niller2005/flupmix/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Niller2005/flupmix/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Niller2005/flupmix/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Niller2005/flupmix/releases/tag/v0.1.0
