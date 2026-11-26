# flupmix

A Voicemeeter-style audio mixer for Linux, built on PipeWire (no JACK) with a
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) interface.
Think jack_mixer for the mixing and qpwgraph for the patching, in one window.

* **Input strips** are either virtual sinks that applications play into (Game, Music, ...) or hardware capture sources (your mic).
* **Buses**: one hardware bus (follows the system default sink, or any sink you pick) and up to eight virtual buses that show up in other apps as microphones (`Audio/Source/Virtual`), e.g. for OBS or Discord.
* Every strip has a fader (-60..+12 dB), mute, global solo, peak meter, a send to the hardware bus and a send to each virtual bus.
* **App routing without lists of every game**: mark one virtual strip as *catch-all* and only list the exceptions. Each strip has an *assigned* list and a *blocked* list (for example block Spotify on Game and assign it to Music). Matching is by process binary or application name.
* **Patchbay** tab: a qpwgraph-like view of the whole PipeWire graph with drag-to-link and click-to-unlink.
* Settings persist in `~/.config/flupmix/config.toml`. Closing the window keeps the mixer running in the system tray (StatusNotifierItem). Without a tray host, closing the window quits.

## Architecture

| crate | role |
| --- | --- |
| `flupmix-core` | Pure model, dB taper, gain matrix and mixing kernel, app-routing rules, TOML config. No audio-server dependency; unit tested. |
| `flupmix-engine` | PipeWire thread: one `pw_filter` mixer node (jack-style single process callback, 16 stereo inputs, 9 stereo outputs), `null-audio-sink` adapters for virtual sinks/sources, link reconciliation, `target.object` metadata for app routing. |
| `flupmix-ui` | GPUI interface. |
| `flupmix` | The binary: wires everything together, system tray. |

The engine is declarative: whenever the mixer state changes it is reconciled
against the live PipeWire registry (create missing nodes, link what should be
linked, move application streams). On exit it gives routed applications back to
the system default.

## Building

Requirements: Rust (stable), `libpipewire` headers (`pipewire` + `clang` for bindgen), a Wayland compositor with Vulkan or GL.

```sh
cargo run --release -p flupmix
```

## Install

`./install.sh` builds a release binary (or uses a prebuilt one from a release tarball) and installs it with its icon and desktop entry under `~/.local` (`PREFIX=/usr/local sudo -E ./install.sh` for system-wide), so flupmix appears in your app menu. `./install.sh uninstall` removes it. Each GitHub release also has a prebuilt x86_64 tarball and an AppImage (`chmod +x flupmix-*.AppImage && ./flupmix-*.AppImage`). The AppImage contains only the binary, so it needs PipeWire and Wayland/X11 plus Vulkan or GL drivers from the host, and a glibc as new as Ubuntu 24.04's.

## Releasing

Update `CHANGELOG.md` (move entries from *Unreleased* into a new `## [x.y.z] - date` section) and bump `version` in the workspace `Cargo.toml`, then push a tag `vx.y.z`. The release workflow checks that tag, version and changelog agree, builds and tests, and publishes the tarball with that changelog section as the release notes. CI (`fmt`, `clippy -D warnings`, tests, release build) runs on every push and pull request.

`cargo run -p flupmix-engine --example headless` runs the engine alone and prints meters, handy for debugging.

## Notes and limits

* Stereo only, mono sources are duplicated to both channels.
* Removing a link that flupmix manages in the patchbay recreates it; use the strip toggles instead.
* Links you create in the patchbay persist after flupmix exits; flupmix's own links and nodes do not.

## Licence

MIT, see [LICENSE](LICENSE). Bundled fonts: IBM Plex Sans (OFL, `assets/fonts/IBMPlexSans-LICENSE.txt`) and Lilex (OFL, `assets/fonts/Lilex-OFL.txt`).
