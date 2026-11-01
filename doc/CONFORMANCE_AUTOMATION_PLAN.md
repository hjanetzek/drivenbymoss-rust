# Conformance runs with Bitwig started per setup

Goal: one command runs the conformance scenarios of one or more controllers unattended. For each
setup it puts the right Bitwig configuration in place, starts Bitwig, runs the scenarios, and
kills Bitwig. No manual controller switching.

## Where Bitwig keeps its state (Linux, 5.3.13)

| What | Path |
| --- | --- |
| Preferences, including the controller list with their ports | `~/.BitwigStudio/prefs/5.3.13.prefs` |
| Unsaved projects which Bitwig restores after a crash or kill | `~/.BitwigStudio/temp-projects/` |
| Lock files of a running instance | `~/.BitwigStudio/lock`, `lock.pid`, `server-port` |
| Settings of the Rust controllers | `~/.config/DrivenByMoss-Bitwig/DrivenByMoss4Reaper-<controller>.config` |
| Extensions | `~/Bitwig Studio/Extensions/` |
| Binary | `/opt/bitwig-studio/bitwig-studio` |

## Profiles

A profile is a snapshot of the Bitwig state for one setup. It is made once, by hand:

1. Configure Bitwig for the setup: the Java controller on `VirMIDI 5-2` (3), the Rust controller
   on `VirMIDI 5-1` (2), the OSC controller, and the fixture project as the default template.
2. Quit Bitwig normally.
3. `tools/bitwig_profile.sh save <name>` copies `5.3.13.prefs` and the DrivenByMoss configs into
   `~/.local/share/dbm-conformance/profiles/<name>/`.

The profiles stay outside the repository: the prefs hold personal settings and paths.

Setups: `launchcontrol_xl`, `maschine_mk2`, `maschine_jam`, later `launchpad`.

## The run script

`tools/conformance_all.sh [setup ...]` (all setups if none are given):

1. **Once at the start:** back up the user's own state (prefs, DrivenByMoss configs). Stop
   dbm-nihost, whose VirMIDI connections clash with the runner's ports. Check that no Bitwig is
   running.
2. **For each setup:**
   1. Copy the profile into place.
   2. Clean up: delete `temp-projects/*` and the lock files (left behind by the last kill), so
      Bitwig starts without the restore dialog.
   3. Start Bitwig with `DBM_OSC_CONFORMANCE=1` and `DBM_DEV_LIBRARY=<main checkout>/rust/target/debug/libdrivenbymoss.so`,
      its log into `target/conformance/bitwig-<setup>.log`.
   4. Wait until it is ready: the OSC controller answers on port 9000 (`/refresh`). Time out
      after about 120 s and record the setup as failed.
   5. Run `conformance_runner.py pair <setup>/*.toml --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-1" --fresh-project 8 --restart 15`.
   6. Copy the replays: `target/conformance/recordings/<run>-rust.rec` and `<run>-java.leds|known`
      into `tests/conformance/<setup>/replay/`.
   7. Kill Bitwig: SIGTERM, then SIGKILL after 10 s. A kill is fine, since every run starts
      with a fresh project.
3. **Once at the end:** restore the user's state, restart dbm-nihost if it was running, and
   run `cargo test --test conformance_replay`. Print a summary per setup: scenarios the same /
   different, replay ok / failed.

`trap` on EXIT restores the user's state and kills Bitwig also when the script is aborted.

## Open questions

- Does Bitwig take the controller list from the prefs alone, or also from somewhere else (a
  cache, the library)? Check with a first profile: copy it in, start Bitwig, compare.
- Does a Bitwig started by the script keep its window and audio engine without a user session
  (display, PipeWire)? It runs in the user's desktop session, so it should.
- Activation and the dashboard: Bitwig must not stop at a dialog. If it does, find the prefs key
  or flag that skips it.
- The fixture as the default template: part of the profile, or set per run?
