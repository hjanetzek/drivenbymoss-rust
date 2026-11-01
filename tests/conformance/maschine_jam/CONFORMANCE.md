# Native Instruments Maschine JAM: conformance with the Java version

The state of the comparison of the Rust port with the Java DrivenByMoss on Bitwig, from the pair
runs of the scenario files in this folder (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md). Updated with every run which changes something.

The runner plays the device in its MIDI mode: buttons are CCs on channel 1, the pads notes 36-99
from the lower left, Shift is the system exclusive message `F0 00 21 09 15 00 4D 50 00 01 4D 01|00
F7`; the setup sends the "return from the host" message (`… 46 01 F7`), on which both versions
redraw everything.

Last full run: 2026-11-01, all five files, on fresh fixture projects with restarted controllers
(`pair ... --fresh-project 8 --restart 15`), Bitwig 5.3.13: all 41 scenarios the same, except
the known differences below.

Replays (`replay/`, run by `cargo test --test conformance_replay`): the Rust recordings with the
final LED state of the Java version.

## Coverage

| File | Manual sections | Scenarios |
| --- | --- | --- |
| `global.toml` | Global, Transport, Group Buttons, Smart Strip Modes | AUTO, Shift + AUTO, Note Repeat, metronome, loop, punch in, tempo, group select / mute / solo / record arm, clear mute / solo, Level |
| `session.toml` | Session Mode | select clip, launch, stop (Mute + pad), flip, scroll |
| `note_sequencer.toml` | Sequencers, Note Sequencer | create notes, Mute + pad, transposition, page, delete |
| `drum_sequencer.toml` | Sequencers, Drum Mode | select drum pad, steps on and off, Mute + pad, Clear + drum pad |
| `play_modes.toml` | Play Modes | Note mode (play, transposition, layout, scale), Chords (triad, dyad), Piano, Drum 64; the played notes (`/track/1/playingNotes`) |

Not covered yet: the Poly, Raindrop and Drum 4/8 sequencers, the loop of the Note Sequencer (two
top row pads), the browser, the encoder modes, the smart strips with values (Macro, Aux,
Control), Duplicate and Clear, the Shift mode, the scene buttons.

## Open differences

None.

## Known differences (not bugs of the port)

| Area | Java | Rust | Why |
| --- | --- | --- | --- |
| Session: the pad of the selected clip | blinks | blinks | emulated blinking (about 600 ms): the last value depends on the phase |
| Session: Shift + arrow (`jam.session.scroll`) | the release of Shift opens the Shift view | stays in the session view | the Shift view stays open only after a tap of Shift (BITWIG_DIVERGENCES.md) |

## Possible Java bugs (to decide later)

| Area | Java | Rust | Java code |
| --- | --- | --- | --- |
| TEMPO + encoder: step | 0.01 BPM | the same | the manual says "Use the encoder in combination with SHIFT for slower changes", but without Shift the step is the slow one; `TempoCommand` uses `isKnobSensitivitySlow`, which Shift sets (`ToggleShiftViewCommand`); in the scenario Shift was used before (Shift + Page Left/Right): a consumed Shift does not get its release, which resets the slow knobs (UPSTREAM_CANDIDATES.md C). Fixed in both versions (2026-11-01); the `tempo +` of the Rust recording `global` was changed by hand to `tempo ++` until the next run |

## Fixed by the comparison

- Chords mode: only the root note of a chord played, and only the pressed pad was lit: the
  extension did not send the further notes of the chord to the note input
  (`MidiInputImpl.sendRawMidiEvent`) and did not report the notes which play on the selected
  track (BITWIG_DIVERGENCES.md §6) (2026-10-30).

- AUTO and Shift + AUTO did nothing on Bitwig: the core had only Reaper's automation modes, the
  extension did not handle `automation`. Now as `TransportImpl` of the Java version (the
  arranger and clip launcher write switches, BITWIG_DIVERGENCES.md §6) (2026-10-30).
- The slow tempo change was 0.1 BPM (Reaper), now 0.01 as the Java version for Bitwig
  (2026-10-30).
- Session: after scrolling by one track the arrow left LED was off: with host paging a bank can
  scroll back from any position > 0, as Bitwig's banks (2026-10-30).
