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
| Layer Mix: Track Focus selects a layer / a drum pad | selects it | nothing | Rust bug, in work |
| Layer Mix: Mute mode, Track Control 1 twice | unmuted again | muted | Rust bug |
| Drum Sequencer: Mute mode, Track Control 2 | pad muted | no change | Rust bug |
| Track Mix: Track Select back to the first page | a track of the page is selected (cursor device and clip of it) | no device, no clip | probably a Rust bug (paging without selection) |
| Equalizer: knob LEDs | type, Q, frequency colors | off | Rust bug |
| Mixer Track Control LEDs after holding Record Arm (transport) | record arm mode (red low) | mute mode (amber low) | probably a Rust bug (mode restored after the temporary transport mode) |
| Template change: LEDs of the previous template | turned off | left on | Rust difference |
| Track Mix: pan knob | no change | pan | probably state: the Device toggle of the Java controller (3rd knob row = device parameters) was on; restart both controllers before a run |
| Equalizer: Send A at the center | off | off | the scenario expects Bell (manual); check the Java code |

## Known differences (not bugs of the port)

| Area | Java | Rust | Why |
| --- | --- | --- | --- |
| Note Sequencer: Track Control row after a template change | track mutes | step mutes | Java bug: the note edit mode is registered in the main and the fader mode manager (BITWIG_DIVERGENCES.md §8); Rust follows the manual |
| Note and drum step knobs (chance etc.): value of an absolute knob | CC / 128 (Bitwig's normalized value of the hardware control, passed to the note parameter) | CC / 127 (as the Java version for Reaper) | at most 0.8 %; Bitwig hardware binding of the Java version |

## Fixed by the comparison

- The master track appeared twice in the Mixer (8th track lit): the extension and the core both
  added it (2026-10-30).
