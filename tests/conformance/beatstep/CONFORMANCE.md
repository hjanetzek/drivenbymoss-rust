# Arturia Beatstep: conformance with the Java version

The state of the comparison of the Rust port with the Java DrivenByMoss on Bitwig, from the pair
runs of the scenario files in this folder (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md).

Status: written 2026-11-10 with the port, **not run on Bitwig yet** (no Bitwig profile
`beatstep`, no Java references, no replays). The Rust core passes all 21 expectations on the
Python reference host (`cd bindings/python && python3 -m conformance beatstep`, which reports "no
reference" until the Java version is recorded).

The runner takes the place of the Beatstep: it sends the pads (notes 36-51 on MIDI channel 3),
the knobs (CC 20-27, 30-37, 40 on MIDI channel 3, relative, offset binary) and the Shift button
(note 7 on MIDI channel 1). The pad LEDs are system exclusive messages
`F0 00 20 6B 7F 42 02 00 10 <pad> <color> F7`, compared per pad (pads 1-8 = 70-77 are the upper
row = grid notes 44-51, pads 9-16 = 78-7F the lower row = grid notes 36-43; colors 00 off, 01
red, 10 blue, 11 pink). Manual: `Arturia/Arturia-Beatstep.md`.

## Setup

In addition to doc/CONFORMANCE_TESTING_PLAN.md (Bitwig with `DBM_OSC_CONFORMANCE=1`, the Java
OSC controller, the fixture project):

1. Add the Java controller "Arturia" → "Beatstep" and the Rust one "Arturia" → "Beatstep (Rust)",
   each with a free VirMIDI port as input and output (`conformance_all.sh` uses VirMIDI 5-2 for
   Java and 5-1 for Rust).
2. Bitwig's takeover mode: the scenarios only use relative knobs, so it does not matter.
3. Quit Bitwig and save the profile: `backends/bitwig/tools/bitwig_profile.sh save beatstep`.
4. Run with the `conformance-run` skill (`conformance_all.sh beatstep`).

## Coverage

| File | Manual section | Scenarios |
| --- | --- | --- |
| `shift.toml` | Shift-Mode | play, repeat, metronome, the transport LEDs, Track and Device Mode selection |
| `track.toml` | Track Mode | track select (with LEDs), record arm, mute/unmute and solo knobs, volume knob, track pages |
| `device.toml` | Device Mode | parameter pages (with LEDs), parameter knob, device on/off (bypass and LED) |
| `session.toml` | Session Mode | the LEDs, scene launch |

Not covered yet: Play Mode (scale, root note, chromatic, octave: only LEDs and the note input,
which needs the probe's played notes), the Drum Sequencer and the Sequencer Mode (steps of a clip),
the Browser Mode, the knobs with exact value changes (volume, pan, sends, parameters), tempo, the
play position (Knob 7 and the big knob), record, tap tempo, inserting devices, group open/close,
adding tracks, entering and leaving layers.

## Open differences

Unknown until the first run.

## Known differences (not bugs of the port)

| Area | Java | Rust | Why |
| --- | --- | --- | --- |
| Note input names in Bitwig | "Control/Pads", "Hardware Sequencer" | "Beatstep", "Beatstep 2" | the Rust extension names the note inputs by the hardware and their index (BITWIG_DIVERGENCES.md §9) |
| Sending the pad LEDs | sleeps 1 ms after each pad sysex | no pause | BITWIG_DIVERGENCES.md §9; if the first run shows lost LED messages on the Rust side, this is the first suspect |
