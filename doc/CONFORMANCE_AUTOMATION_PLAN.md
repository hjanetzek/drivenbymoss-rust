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
| Binary (the user's, with JDK 25) | `~/.local/opt/bitwig-studio-jdk25/bitwig-studio` (`BITWIG`) |

## Profiles

A profile is a snapshot of the Bitwig state for one setup. It is made once, by hand:

1. Configure Bitwig for the setup: the Java controller on `VirMIDI 5-2` (3), the Rust controller
   on `VirMIDI 5-1` (2), the OSC controller, and the fixture project as the default template.
2. Quit Bitwig normally.
3. `backends/bitwig/tools/bitwig_profile.sh save <name>` copies `5.3.13.prefs` and the
   DrivenByMoss configs into `~/.local/share/dbm-conformance/profiles/<name>/`. The name is the
   folder of the setup in `tests/conformance/`.

The profiles stay outside the repository: the prefs hold personal settings and paths.

Setups: `launchcontrol_xl`, `maschine_mk2`, `maschine_jam`, later `launchpad`.

## The run script

`backends/bitwig/tools/conformance_all.sh [--no-replays] [setup ...]` (all setups with a profile
if none are given). Implemented as below; Bitwig's readiness is
`conformance_runner.py wait` (the OSC controller answers and the two MIDI ports exist).

1. **Once at the start:** check that no Bitwig is running, build the library (`cargo build`),
   back up the user's own state (prefs, DrivenByMoss configs; unsaved projects are not kept,
   Bitwig on this computer is only for development and tests). Stop dbm-nihost, whose VirMIDI connections clash with
   the runner's ports.
2. **For each setup:**
   1. Copy the profile into place.
   2. Clean up: delete `temp-projects/`, `undo-history/`, `view-settings/` and the lock files
      (left behind by the last kill), so Bitwig starts without the dialog "Recover Projects".
      `temp-projects/` alone was not enough (first run, 2026-11-01).
   3. Start Bitwig with `DBM_OSC_CONFORMANCE=1` and `DBM_DEV_LIBRARY=<main checkout>/rust/target/debug/libdrivenbymoss.so`,
      its log into `target/conformance/bitwig-<setup>.log`.
   4. Wait until it is ready: the OSC controller answers on port 9000 (`/refresh`) and the two
      MIDI ports exist, then 10 s for the controllers. Time out after 180 s and record the setup
      as failed.
   5. Run `conformance_runner.py pair <setup>/*.toml --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-1" --fresh-project 8 --restart 15`.
   6. Copy the replays: `target/conformance/recordings/<run>-rust.rec` and `<run>-java.leds|known`
      into `tests/conformance/<setup>/replay/`, per scenario file and only if that file had no
      DIFF, MISSING or failed expectation in the run log, and only if the copied recording
      replays (`DBM_REPLAY=… cargo test --test conformance_replay`); otherwise the committed
      files are restored. The summary lists the copied and the skipped files.
   7. Kill Bitwig: SIGTERM, then SIGKILL after 10 s. A kill is fine, since every run starts
      with a fresh project.
3. **Once at the end:** restore the user's state, delete `temp-projects/`, restart dbm-nihost if
   it was running, and run `cargo test --test conformance_replay`. Print a summary per setup:
   scenarios the same / different, expectations, replay ok / failed. Logs:
   `target/conformance/bitwig-<setup>.log`, `run-<setup>.log`.

`trap` on EXIT restores the user's state and kills Bitwig also when the script is aborted.

## Running by an agent

Once the profiles exist, a subagent may run the conformance tests with this script (and only
with it): it starts and kills its own Bitwig. Drafts for Claude Code: the subagent
`.claude/agents/conformance-runner.md` (the rules and the report) and the skill
`.claude/skills/conformance-run/SKILL.md` (preconditions, installing, judging results, replays).
The brief:

- The user's Bitwig must not run; if it does, stop and report, do not kill it.
- Run `rust/backends/bitwig/tools/conformance_all.sh <setups>` in the background (a setup takes
  about 2 to 6 minutes) and wait for it.
- Report the summary; for `DIFF` scenarios and failed expectations the lines from
  `target/conformance/run-<setup>.log`; whether the replays changed (`git status
  tests/conformance`). Do not commit; do not edit scenario files to make a run pass.

## First run (2026-11-01)

- The prefs alone carry the controllers; Bitwig started in the desktop session and became ready
  for all three setups; no activation or dashboard dialog.
- The dialog "Recover Projects" appeared after a kill: now `undo-history/` and `view-settings/`
  are deleted too (not yet confirmed by a run).
- Mk2 and Launch Control XL: 27 / 27 the same. JAM: differences in `global` (the tempo: the Java
  fix of upstream candidate C was not installed), `note_sequencer` and `session` (the Java capture
  lost the CC LEDs), maybe caused by the restored projects; to run again.
- The script copied all replays, also of the files with differences: now only clean files that
  replay are copied.
- Second JAM run: `global` (tempo 121 in both), `session` the same, no recovery dialog;
  `note_sequencer` lost the Java CC LEDs. Cause: the runner dropped the output before the setup
  and relied on its full redraw, which is not complete in the Java version on Bitwig
  (`HwLightImpl.forceFlush` turns a light off and restores it 100 ms later; Bitwig sends only
  changes, so LEDs which are off are not sent). The runner now starts the LED state with the
  startup output of the restarted controller (`restart_controller` returns it).
- Run with clean profiles (all three setups): the startup output broke the Mk2 (Rust's host query
  to dbm-nihost) and the Launch Control XL (Java's LEDs on channel 16 before the template select),
  0 of 27 the same each. Keeping the startup output is now an option of the scenario file
  (`keep_startup_output = true`, only the JAM files). The JAM session started with the transport
  still playing from the Java run: the runner now sends `/stop` twice after the new project
  (stop, then rewind; `/play 0` of the OSC controller calls Bitwig's `play ()` and starts it again).
- Damaged MIDI messages (aseqdump run, `DBM_ASEQDUMP=1`): `aseqdump` sees the same damaged event as
  the runner (`B0 00 53` for `B0 53 4D`, as if a `00` was inserted after the status byte), while
  the extension's recording has the right one: the damage happens in Bitwig's MIDI output or in
  snd-virmidi, before the ALSA sequencer, not in rtmidi. Next: `strace -e write` on Bitwig's
  `/dev/snd/midiC5D*` to tell Bitwig and the kernel apart.
- The double `/stop` came too early: Bitwig resumes the old play state when the audio engine runs
  again, after the `/stop`s. The runner now waits for the engine and stops until Bitwig reports
  `/play 0`, then rewinds.
- The engine wait was a fixed pause, then fixed: the runner waits until Bitwig reports
  `/project/engine 1`. JAM session the same. The transport also played at a controller restart
  (the old project, before the new one): the JAM strips then switch to dual mode (the volume as
  dual value, `MaschineJamVolumeMode` while playing) and that is in the startup output the JAM
  keeps. The runner now also stops the transport before the restart (`stop_transport`).
