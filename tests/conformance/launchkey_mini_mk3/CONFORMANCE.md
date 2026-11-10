# Novation Launchkey Mini Mk3: conformance with the Java version

The scenario files of the Launchkey Mini Mk3 (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md). **Not run yet** on Bitwig: there is no Bitwig profile of this setup. The expectations pass with
the Python reference host (`bindings/python`, `python3 -m conformance <setup>` with a temporary
`Setup` entry for it in `conformance/fixture.py`; checked 2026-11-10); without Java references
there is nothing to compare yet.

The runner plays the DAW port of the device: the buttons and knobs are CCs on channel 16, the
scene buttons CCs 104/105 on channel 1, the pads in the session pad mode notes 96-103 (upper row)
and 112-119 (lower row) on channel 1. The stub echoes the DAW mode note.

## What the user needs to set up

1. A Bitwig profile `launchkey_mini_mk3` (`backends/bitwig/tools/bitwig_profile.sh save
   launchkey_mini_mk3`) with both controllers on the virtual ports of the runner:
   - Java DrivenByMoss "Novation Launchkey Mini Mk3" on `VirMIDI 5-2` (input 2, the keyboard
     port: any free virtual port),
   - DrivenByMoss (Rust) "Novation Launchkey Mini Mk3" on `VirMIDI 5-1` (input 2: another free
     port),
   - the Java OSC controller and the probe extension as for the other setups,
   - takeover mode "Immediate".
2. The fixture project of FIXTURE.md.

Then run it with the `conformance-run` skill (setup `launchkey_mini_mk3`).

## Coverage

| File | Manual sections | Scenarios |
| --- | --- | --- |
| `session.toml` | Session Mode, Volume Mode | launch, the Record Arm sub-mode, the metronome of the mode selection view, knob volume, Track Right |

Not covered yet: the drum mode, the custom (user) knob mode on the keyboard port (the runner
drives one port), the program changes.
