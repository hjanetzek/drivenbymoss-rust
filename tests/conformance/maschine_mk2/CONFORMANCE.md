# Native Instruments Maschine Mk2: conformance with the Java version

The state of the comparison of the Rust port with the Java DrivenByMoss on Bitwig, from the pair
runs of the scenario files in this folder (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md). Updated with every run which changes something.

The runner plays the device in its MIDI mode: buttons are CCs on channel 1, the pads notes 36-51
from the lower left; the Mk2 has no Shift in MIDI mode, STEP LEFT (the Stop CC 59) is Shift. The
display is the MCU text display (`F0 00 00 66 14 12 <position> <text> F7`), compared per
position (`sysex_key_bytes = 7`).

Last full run: 2026-11-08 with the extension on the binary protocol (typed protocol step 3,
also once with `DBM_HOST_PROTOCOL=check`: 0 differences to the text), unattended
(`conformance_all.sh`); before: 2026-11-01, on fresh fixture projects with restarted controllers (`pair ...
--fresh-project 8 --restart 15`), Bitwig 5.3.13: all 27 scenarios of both files the same, except
the known differences below.

Replays (`replay/`, run by `cargo test --test conformance_replay`): the Rust recording with the
final LED state of the Java version.

## Coverage

| File | Manual sections | Scenarios |
| --- | --- | --- |
| `pad_modes.toml` | Pad Modes, Drum Mode, Play Mode | Chords, Drum and Play (keyboard) via PAD MODE with the played notes, the drum step sequencer (step on/off, Mute + pad), Pattern + Select, Scene + Select, Variation |
| `global.toml` | Mk2, Global, Transport, Group buttons, Encoder modes | AUTO, Shift + AUTO, LOCK, Shift + LOCK (overdub), Note Repeat, loop, metronome (Shift + TAP), group select / mute / solo / record arm, clear mute / solo, the buttons above the display (Volume, Panorama, Send +, Parameters), tempo |

Not covered yet: the Events mode, the grid configuration (Follow), the configuration pages (Shift
+ PAD MODE), the browser, the knobs, the touch strip, Erase and Duplicate, the Shift view
functions on the pads.

## Open differences

None.

## Known differences (not bugs of the port)

| Area | Java | Rust | Why |
| --- | --- | --- | --- |
| VU meters | | | audio levels |
| Variation mode: the pad of the selected parameter (pad 5 before pad 2 is pressed, then pad 2) | blinks | blinks | emulated blinking: the last value depends on the phase |
| The labels of the buttons above the display (`F0 7D 44 42 4D 10 00 …`) | - | sent | an addition of the Rust version for dbm-nihost (BITWIG_DIVERGENCES.md §6) |
| The query whether the host sends the Shift button (`F0 7D 44 42 4D 11 F7`, at the startup) | - | sent | an addition of the Rust version for dbm-nihost (BITWIG_DIVERGENCES.md §6); not a LED, the runner keys it as `F0 7D 44 42 4D`, which the Java file does not have. |

## Fixed by the comparison

- The values on the display (e.g. panorama `0.00 %` instead of `0.00`): Java gets the texts
  shortened by Bitwig (`getPanStr (6)`, `getLimited`, which shortens in its own way), Rust cut
  its full text. The extension now reports Bitwig's texts for 6 and 8 characters
  (BITWIG_DIVERGENCES.md §6) (2026-10-30).
- LOCK and Shift + LOCK (overdub) did nothing on Bitwig: the core had only Reaper's overdub of
  the selected track. Now Bitwig's arranger and clip launcher overdub switches, as
  `TransportImpl` (BITWIG_DIVERGENCES.md §6) (2026-10-30).
- Shift (STEP LEFT) + LOCK / MUTE / SOLO left the Shift view of the pads while Shift was still
  held: the extension reports the values of a track again after a command to it, and the core
  treated the repeated `select 1` as a selection change (recalled the preferred view). A
  selection is now only handled when it changes (2026-10-30).
- A notification on the display lasted about 1.1 s instead of 700 ms: the checks were counted,
  but each only ran with a tick. It now ends at its time (2026-10-30).
- The Pin button was lit in the Parameters mode: the cursor device counted as always pinned (as
  in Reaper); on Bitwig it is the pinned state of the cursor device (2026-10-30).
