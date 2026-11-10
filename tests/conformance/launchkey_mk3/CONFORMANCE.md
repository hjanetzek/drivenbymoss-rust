# Novation Launchkey Mk3: conformance with the Java version

The scenario files of the Launchkey Mk3 (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md). **Not run yet** on Bitwig: there is no Bitwig profile of this setup. The expectations pass with
the Python reference host (`bindings/python`, `python3 -m conformance <setup>` with a temporary
`Setup` entry for it in `conformance/fixture.py`; checked 2026-11-10); without Java references
there is nothing to compare yet.

The runner plays the DAW port of the device: the buttons and knobs are CCs on channel 16, the
scene buttons CCs 104/105 on channel 1, the pads in the session pad mode notes 96-103 (upper row)
and 112-119 (lower row) on channel 1. The device sends the selected knob, fader and pad modes as
CC 9, 10 and 3 on channel 16 (Shift + pads); the scenarios send them directly. The stub echoes
the DAW mode note and answers the device inquiry as a Launchkey 49 (with faders). The setup
sends the DAW mode note once more, since the restart of the controller runs without the stub,
and the Mk3 only updates the device after it was confirmed.

## What the user needs to set up

1. A Bitwig profile `launchkey_mk3` (`backends/bitwig/tools/bitwig_profile.sh save launchkey_mk3`)
   with both controllers on the virtual ports of the runner:
   - Java DrivenByMoss "Novation Launchkey Mk3" on `VirMIDI 5-2` (input 1 and output: DAW port;
     input 2, the keyboard port: any free virtual port, e.g. `VirMIDI 5-3`),
   - DrivenByMoss (Rust) "Novation Launchkey Mk3" on `VirMIDI 5-1` (input 2 e.g. `VirMIDI 5-0`),
   - the Java OSC controller and the probe extension as for the other setups,
   - takeover mode "Immediate" (the knobs and faders are absolute).
2. The fixture project of FIXTURE.md.

Then run it with the `conformance-run` skill (setup `launchkey_mk3`).

## Coverage

| File | Manual sections | Scenarios |
| --- | --- | --- |
| `global.toml` | Global Features, Knob modes, Fader modes | Click, Repeat, Play, Stop, Track Left/Right, fader volume, the buttons below the faders (select, arm), knobs in the Pan and Volume modes |
| `session.toml` | Session Mode | launch, Stop + pad, the Mute, Solo and Track Select sub-modes |

Not covered yet: the display (sysex, compared as LEDs per message type), the drum mode and its
sequencer, the device select mode, the browser, Shift + Play (known difference: no host command
in the Rust core, BITWIG_DIVERGENCES.md §9), the custom modes.

## Expected differences

- The base screen of the parameter mode: Java shows `Optional[...]` for the page (upstream
  candidate O).
