# Novation Launch Control XL: conformance with the Java version

The state of the comparison of the Rust port with the Java DrivenByMoss on Bitwig, from the pair
runs of the scenario files in this folder (see doc/CONFORMANCE_TESTING_PLAN.md and
tests/conformance/FIXTURE.md). Updated with every run which changes something.

Last full run: 2026-10-30, all five templates, each recording on a fresh fixture project
(`pair ... --fresh-project 8`), Bitwig 5.3.13.

## Coverage

| File | Template | Scenarios |
| --- | --- | --- |
| `track_mix.toml` | 1 Track Mix | track select, mute/solo/record arm modes, knobs and faders, device page, track paging, transport (click, tempo+, mute/solo defeat) |
| `equalizer.toml` | 2 Equalizer | band type; Q, frequency, gain |
| `layer_mix.toml` | 3 Layer Mix | layer select, layer mute, layer volume/pan, drum pads of a Drum Machine |
| `drum_sequencer.toml` | 4 Drum Sequencer | pad select, step on/off, step knobs, pad mute, pad paging |
| `note_sequencer.toml` | 5 Note Sequencer | device page, note create/delete, repeat, panning |

Not covered yet: the Device and Send Select combinations of the Track Mix (parameter modes, send
pages), the Solo + Track Control scene launch, faders with a held Track Focus (send and master
volumes), the long press of the step resolution (Drum Sequencer Mute), loop length (Record Arm),
the remaining transport buttons.

## Same in both

Track select, mute, solo and record arm modes; sends, faders; device pages of the cursor device
and the first instrument; track paging; click, tempo and mute/solo defeat; the Note Sequencer
(device page, notes, repeat, panning) and the steps of the Drum Sequencer.

## Open differences

| Area | Java | Rust | Status |
| --- | --- | --- | --- |
| Equalizer: knob LEDs | type, Q, frequency colors | off | Rust bug |
| Template change: LEDs of the previous template | turned off | left on | Rust difference |
| Equalizer: Send A at the center | off | off | the scenario expects Bell (manual); check the Java code |
| Drum Sequencer (Java run): Track Control row | arms tracks for recording | steps | the Java bug of the Note Sequencer (below) also hits the Drum Sequencer (`drumPadEditMode` registered twice); the Java controller has to be restarted before a run, otherwise its Drum Sequencer scenarios fail |

## Known differences (not bugs of the port)

| Area | Java | Rust | Why |
| --- | --- | --- | --- |
| Note Sequencer: Track Control row after a template change | track mutes | step mutes | Java bug: the note edit mode is registered in the main and the fader mode manager (BITWIG_DIVERGENCES.md §8); Rust follows the manual |
| Note and drum step knobs (chance etc.): value of an absolute knob | CC / 128 (Bitwig's normalized value of the hardware control, passed to the note parameter) | CC / 127 (as the Java version for Reaper) | at most 0.8 %; Bitwig hardware binding of the Java version |
| Layer Mix with an Instrument Layer of 2 layers: Track Select "previous" LED | on | off | there is no previous page; Java takes it from Bitwig's layer bank (`canScrollPageBackwards`), Rust from its paging over all layers |

## Fixed by the comparison

- Track Mix: knob LEDs after turning a knob showed the value sent, also when Bitwig did not take
  it (in the fixture, Bitwig keeps track volume and sends, also from OSC; Bitwig's takeover
  mode is set to catch).
  The Java version shows Bitwig's value. The extension now reports the values of the addressed
  channel again 300 ms after a value command, so the LEDs fall back to Bitwig's value
  (BITWIG_DIVERGENCES.md §6). This also explained the pan difference (2026-10-30).
- Track Mix: Track Select to the previous page selected the last track of the page and moved the
  selection on every page scroll, as the Java version for Reaper; with host paging (Bitwig) the
  first track is selected and scrolling only scrolls, as the Java version for Bitwig
  (`AbstractItemBankImpl`). Applies to all banks (2026-10-30).
- Track Mix: the track button mode (Mute, Solo, Record Arm) fell back to Mute when the parameters
  of the device changed (e.g. after selecting another track): the app activates the mode again to
  rebind the knobs (as `rebindKnobs` of the Java version for Reaper, which has the same effect
  there). The main modes now keep the track button mode on a repeated activation (2026-10-30).
- Layer Mix: Track Focus did not select layers (the layer channel had no `select`, now
  `primary layer/<i>/select`), and with a Drum Machine it addressed the pads from pad 0 instead
  of C1 (a drum pad bank reported by the host starts at C1 like Bitwig's, and keeps a scroll
  position requested before the pads arrived exactly instead of aligned to the page). This also
  fixed the layer mute (toggled twice) and the pad mute of the Drum Sequencer (2026-10-30).
- The master track appeared twice in the Mixer (8th track lit): the extension and the core both
  added it (2026-10-30).
