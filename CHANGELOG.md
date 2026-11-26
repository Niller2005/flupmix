# Changelog

All notable changes to flupmix are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The release workflow publishes the
section matching the pushed tag as the GitHub release notes.

## [Unreleased]

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

[Unreleased]: https://github.com/Niller2005/flupmix/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Niller2005/flupmix/releases/tag/v0.1.0
