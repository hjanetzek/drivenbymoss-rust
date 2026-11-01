---
name: conformance-run
description: How to run the DrivenByMoss conformance tests (Java vs. Rust on Bitwig) and handle the results - when the user asks to run the conformance tests or a setup (Mk2, JAM, Launch Control XL), to record replays again, or after changes which need a conformance check. Delegates the run to the conformance-runner subagent; covers the preconditions, installing the Java extension, judging differences, replays and commits.
---

<!-- DRAFT (2026-11-01): written from the first unattended runs; refine in later sessions. -->

# Conformance runs

The conformance tests play the same MIDI into the Java DrivenByMoss and the Rust port on Bitwig
and compare LEDs, displays and Bitwig's state. Background: `doc/CONFORMANCE_TESTING_PLAN.md`,
`doc/CONFORMANCE_AUTOMATION_PLAN.md`, per controller `tests/conformance/<setup>/CONFORMANCE.md`.

## Before a run

- **Profiles:** `backends/bitwig/tools/bitwig_profile.sh list` shows the setups which can
  run. The user makes them by hand (Bitwig configured, quit, `bitwig_profile.sh save <setup>`).
- **Bitwig closed:** the script refuses otherwise. Ask the user to quit Bitwig; never kill it.
- **The user not at the controllers:** the script stops dbm-nihost and drives the VirMIDI ports.
- **Builds current:** the script runs `cargo build` (the dev library). The Rust extension and the
  Java extension are installed by hand and only when the user asks:
  - Rust extension: `cd backends/bitwig && mvn -o -q install`.
  - Java (`~/bastel/github/DrivenByMoss`, branch `conformance-osc`): `mvn -o -q package -DskipTests`,
    then `cp target/DrivenByMoss-26.7.0.jar ~/"Bitwig Studio/Extensions/DrivenByMoss.bwextension"`
    (never `mvn install` there; the backup `target/DrivenByMoss.bwextension.before-conformance`
    stays untouched).
  A fix on one side only (e.g. a Java fix not installed) shows as a difference; say so in the
  brief.

## Running

Start the `conformance-runner` subagent in the background with the setups and a short brief:
what changed since the last run, what is expected, what is under investigation. It runs
`conformance_all.sh`, waits, and reports. While it runs, the user must not start Bitwig or click
Bitwig windows (a closed dashboard may have caused a damaged MIDI message once).

## Judging the results

- **Same everywhere:** the script has copied the replays of all files; the replay test passed.
- **DIFF on one side only (`None | …`):** first suspect the MIDI path, not the port: the rare
  damaged message (CONFORMANCE_AUTOMATION_PLAN.md, "First run"). One rerun of the setup usually
  clears it.
- **DIFF with values on both sides:** a real difference. Check the Java original and the manual
  (DrivenByMoss-Documentation) before changing the port; a deliberate divergence goes to
  `doc/BITWIG_DIVERGENCES.md` and, if it shows in a scenario, a `[[known_difference]]` in the
  scenario file, with the reason, and a row in the controller's CONFORMANCE.md.
- **A bug of the Java version:** `doc/UPSTREAM_CANDIDATES.md`; a fix in Java only on the branch
  `conformance-osc`, listed in its `CONFORMANCE-OSC.md`.
- **FAIL (expectation):** in both versions it is usually the scenario or the Java original; in the
  Rust version only, a port bug.

## Replays and commits

- The script copies a file's replays only from a clean run of it that replays; other files keep
  their committed replays. Check `git status tests/conformance`.
- A behaviour change of the port changes the recorded outputs: a replay may then fail until the
  next run records it again. Prefer a new run; a hand edit of a recording only as a stopgap, noted
  in the controller's CONFORMANCE.md.
- Commit only when the user asks: the replays with a message naming the run and its result, and
  update "Last full run" in the controller's CONFORMANCE.md.
