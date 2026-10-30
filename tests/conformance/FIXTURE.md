# Conformance fixture project

The scenarios of all controllers run on one Bitwig project with this content. Create it once,
save it (e.g. as `conformance.bwproject`) and open it before a run. The scenario files select
the track they need and reset the states they change via OSC (their `setup` and `before`
steps), and check the content with `require` before they start.

Tracks, in this order, all in the root (no groups), not muted, not soloed, not armed:

| # | Name | Type | Content |
|---|---|---|---|
| 1 | Synth | Instrument | Polysynth (at least 2 remote control pages); an empty launcher clip of 1 bar in slot 1 |
| 2 | Drums | Instrument | Drum Machine with at least 8 pads with a device each, starting at C1 (e.g. a kit); an empty launcher clip of 1 bar in slot 1 |
| 3 | Layers | Instrument | Instrument Layer with 2 layers, each with a Polysynth |
| 4 | EQ | Audio | EQ+ as the first device |
| 5–10 | Audio 5 … Audio 10 | Audio | empty |

Effect tracks: `FX 1` and `FX 2` (the sends A and B of the Launch Control XL).

The Java OSC controller sends to 127.0.0.1:9000 and receives on 8000; its MIDI port setting can
be any unused device. See doc/CONFORMANCE_TESTING_PLAN.md for the rest of the setup.
