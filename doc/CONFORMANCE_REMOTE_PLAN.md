# Conformance runs on other machines

Goal: run the conformance setups (`conformance_all.sh`, see CONFORMANCE_AUTOMATION_PLAN.md) on
other machines over ssh, in parallel, without a person at them. A full run of the three setups
takes about 20 minutes on one machine; one setup per machine brings it down to the longest
setup (the JAM, about 8 minutes). Nothing here is tested yet.

## Constraint: the runner runs next to Bitwig

The runner talks to Bitwig over VirMIDI (`snd-virmidi`) and OSC on localhost (ports 8000 and
9000). So each machine runs Bitwig, the runner and `conformance_all.sh` itself; only the
orchestration is remote.

## Options for starting Bitwig without a person

| Option | How | For | Against |
| --- | --- | --- | --- |
| A. The logged-in user's desktop | ssh, then `systemd-run --user --wait …` (or `machinectl shell`), which starts the process inside the user's session with `WAYLAND_DISPLAY`/`DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS` and PipeWire; or export these variables by hand | Bitwig runs exactly as when started by hand; audio and OpenGL as usual | A desktop session must be logged in (autologin); a screen locker must not get in the way |
| B. Headless | a virtual display (Xvfb or a headless Wayland compositor) and a PipeWire for the user, started by a systemd user service | No desktop session needed, also for VMs and servers | Unknown whether Bitwig's UI (OpenGL through Mesa llvmpipe) and audio engine run there; to be tried |
| C. Several instances on one machine | separate users or HOMEs (avoids the single-instance lock in `~/.BitwigStudio`) | One machine | The OSC ports (Java OSC controller, runner) and the VirMIDI ports would have to be configurable per instance; CPU and audio contention between instances |

**Recommendation:** start with A on one remote machine (the session is KDE on Wayland) and check
that `systemd-run --user` brings up Bitwig with its window and audio. Try B only if a machine has
no desktop session. C is not worth the configuration work while separate machines or VMs exist.

## What each machine needs

- **Bitwig 5.3.13**, activated. A license allows a limited number of computers; check the count
  before setting up more machines.
- **The kernel module** `snd-virmidi` (`packaging/` of dbm-nihost loads it at boot).
- **The two extensions:** the Java `conformance-osc` build and the Rust extension, in
  `~/Bitwig Studio/Extensions/`.
- **The repository checkout** with the built library (`cargo build`, used through `DBM_DEV_LIBRARY`),
  and `uv` for the runner.
- **The profiles** in `~/.local/share/dbm-conformance/profiles/` and the fixture project as the
  default template. Profiles are per machine: the prefs hold the audio device and the MIDI port
  names, so they are made on each machine once (`bitwig_profile.sh save`), not copied.
- **No dbm-nihost** competing for the VirMIDI ports, or the script stops it (it does).

## Orchestration

A wrapper `conformance_remote.sh` on the main machine:

1. **Map hosts to setups,** e.g. `host1=maschine_jam host2=maschine_mk2 host3=launchcontrol_xl`
   (a small config file).
2. **Sync each host** to the commit under test: `git fetch && git checkout <commit>`, then
   `cargo build` and the extension builds (or rsync the built artifacts).
3. **Run in parallel:** `ssh host 'systemd-run --user --wait --pipe … rust/backends/bitwig/tools/conformance_all.sh <setup>'`,
   with the output in a log per host.
4. **Collect** `target/conformance/run-<setup>.log`, the traces and the changed replay files
   (rsync, or `git diff` from each host).
5. **Merge** the replays into the main checkout and run `cargo test --test conformance_replay`
   once there. Print one summary for all hosts.

An agent can run the wrapper the same way as `conformance_all.sh` today (CONFORMANCE_AUTOMATION_PLAN.md,
"Running by an agent").

## Steps

1. One remote machine: install as above, make one profile, start `conformance_all.sh` by hand over
   ssh with option A. Check the window, the audio, the ready check and the summary.
2. Write `conformance_remote.sh` for one host, then for several in parallel.
3. Set up the other machines; split the setups.
4. Optional: option B for a machine without a desktop.

## Open questions

- Does `systemd-run --user` give Bitwig the Wayland session on KDE, or does it need the variables
  of the running session (`systemctl --user show-environment`)?
- Does a locked screen or a sleeping display stop Bitwig's UI or engine?
- How many activations does the license allow?
- Does the rare damaged MIDI message (CONFORMANCE_AUTOMATION_PLAN.md, "First run") also occur on
  other machines? A comparison between machines would help to find its layer.
