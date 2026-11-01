# Upstream candidates

This document lists the changes that could become pull requests to upstream projects. Part 1 covers
bug fixes for the Java DrivenByMoss (Bitwig). Part 2 plans a standalone repository for dbm-nihost.

As of 2026-10-31. The Java line numbers refer to upstream `origin/master` of
`~/bastel/github/DrivenByMoss` (4bbb242b), not to the local branches. Nothing here has been sent.

## Part 1: Bug fixes for the Java DrivenByMoss

### 1.1 Where the changes are

The Java repository has two local branches. Both start at `origin/master` (4bbb242b):

- `linux-maschine`: one commit, 219204b8.
- `conformance-osc`: the commit of `linux-maschine`, plus seven commits for the conformance tests.

The changes of `conformance-osc`, sorted:

| Commit | Change | Kind |
| --- | --- | --- |
| 219204b8 | Maschine Mk2/JAM: remove the Linux check, add Linux discovery pairs for the JAM | real change, candidate J |
| e8eb02a9 | `ConformanceModule` (OSC), `/clip/step/…`, `/track/N/playingNotes` | test only |
| 6957325b | OSC `/device/layer/N/browse`, `…/addDevice`; `IChannel.addDevice (metadata, evenIfMissing)` | test only (maybe useful, see 1.5) |
| b6c21fbe | `CONFORMANCE-OSC.md` | test only |
| 4fc63753 | `/conformance/action`, `/conformance/actions`; `IApplication.getActionDescriptions` | test only (maybe useful, see 1.5) |
| 062cefbe | `/conformance/device/slots` | test only |
| 4efbc75d | `/conformance/primary/layer/…`, `…/drumpad/…` | test only |
| d66bb744 | `XLNoteSequencerMode`: no exception for a step without a note | real bug fix, candidate I |
| 4d3653e4 | `MidiInputImpl`: restart sysex `F0 7D 44 42 4D 03 F7` with `DBM_OSC_CONFORMANCE` | test only |

Notes:

- 219204b8 cherry-picks cleanly onto `origin/master`.
- d66bb744 also touches `CONFORMANCE-OSC.md`, which does not exist upstream. Without that hunk it
  applies cleanly.
- Many Java files use CRLF line endings (`TrackImpl`, `CursorDeviceImpl`, `StepInfoImpl`,
  `CursorClipImpl`, `ToggleShiftViewCommand`, `XLEqMode`, `XLAbstractMainMode`). A patch must keep
  them. `LaunchControlXLControllerSetup` uses LF.

### 1.2 Sources

- `rust/doc/BITWIG_DIVERGENCES.md` §6 and §8.
- `rust/doc/BITWIG_CORE_PORT_ANALYSIS.md` (Track, CursorDevice, CursorClip sections).
- The "Possible Java bugs" tables in `rust/tests/conformance/*/CONFORMANCE.md`. The "Fixed by the
  comparison" entries are all Rust bugs, except the EQ activation (candidate H).
- `git log` on `rust-port`: 4f41d6e6, 0b69c81c, 566e4a9b.
- The manual: `~/bastel/github/DrivenByMoss-Documentation`.
- Reading the Java code around these places found candidate C (the real cause of the JAM tempo
  step), D and F.

### 1.3 The candidates

#### A. `TrackImpl.setMonitor (false)` switches monitoring on

- **Where:** `src/main/java/de/mossgrabers/bitwig/framework/daw/data/TrackImpl.java:261-264`.
- **What is wrong:** the method ignores its value and always sets `MONITOR_MODE_ON`.
- **Reproduce:** with the OSC controller, send `/track/1/monitor 0` (`TrackModule.java:443-448`).
  Monitoring of track 1 goes on, not off. Generic Flexi "Track N: Set Monitor" calls it too
  (`TrackHandler.java:584, 594`), but only with a pressed button, so it always passes true.
- **Evidence:** `BITWIG_CORE_PORT_ANALYSIS.md` (Track section), `BITWIG_DIVERGENCES.md` §6
  ("Tracks and channels"). The port does not copy it. `setAutoMonitor` next to it does it right.
- **Fix:**

  ```diff
       public void setMonitor (final boolean value)
       {
  -        this.track.monitorMode ().set (MONITOR_MODE_ON);
  +        this.track.monitorMode ().set (value ? MONITOR_MODE_ON : MONITOR_MODE_OFF);
       }
  ```

- **Confidence:** high. A bug.
- **Ready:** yes. It can be checked with OSC, without a controller.

#### B. `CursorDeviceImpl.selectPrevious` at the first device of a page

- **Where:** `src/main/java/de/mossgrabers/bitwig/framework/daw/data/CursorDeviceImpl.java:109-115`.
- **What is wrong:** at the first device of a page (`getIndex () == 0`), the cursor moves to the
  previous device. Then `deviceBank.selectPreviousPage ()` scrolls back and, 75 ms later, selects
  item 0 of the new page (`AbstractItemBankImpl.java:195-201`). From device 9 you end on device 1,
  not on device 8. `selectNext` has the same pattern, but there item 0 of the next page is the
  right device.
- **Reproduce:** a track with at least 9 devices. Select device 9. Press "previous device", for
  example Maschine JAM parameter mode, Mk2 PageCommand, Launch Control XL Device + Track Select
  left (about 20 controllers call it). The selection jumps to device 1.
- **Evidence:** `BITWIG_DIVERGENCES.md` §6 ("Device siblings", the note at the end),
  `BITWIG_CORE_PORT_ANALYSIS.md` (CursorDevice, finding 3). The port copies the Java behaviour.
- **Fix:** scroll the bank without selecting. `scrollPageBackwards` is protected in
  `AbstractItemBankImpl`, so use `scrollTo`:

  ```diff
       public void selectPrevious ()
       {
  -        final boolean moveBank = this.getIndex () == 0;
  +        final int position = this.getPosition ();
  +        final boolean moveBank = this.getIndex () == 0 && position > 0;
           this.cursorDevice.selectPrevious ();
           if (moveBank)
  -            this.deviceBank.selectPreviousPage ();
  +            // Show the previous page, keep the device which the cursor selected
  +            this.deviceBank.scrollTo (position - this.deviceBank.getPageSize ());
       }
  ```

- **Confidence:** high for the cause, from the code. A bug.
- **Ready:** after a check in Bitwig (any controller, or OSC `/device/-`).

#### C. A consumed Shift leaves the knobs slow (the JAM tempo step)

- **Where:**
  - `src/main/java/de/mossgrabers/framework/controller/hardware/AbstractHwButton.java:103-117`
    (`handleButtonRelease`): the UP event does not reach the command if the button was consumed.
  - `src/main/java/de/mossgrabers/framework/command/trigger/view/ToggleShiftViewCommand.java:53-73`:
    it sets the slow knob sensitivity on DOWN and clears it only on UP.
  - Many JAM commands call `setTriggerConsumed (ButtonID.SHIFT)`, for example
    `MaschineJamPageLeftCommand.java:45`; also `EncoderModeManager.java:235`.
- **What is wrong:** after any Shift combination (Shift + PAGE LEFT, Shift + AUTO, …), releasing
  Shift does not call `setKnobSensitivityIsSlow (false)`. All knobs stay slow until the next plain
  Shift press. The TEMPO encoder then steps 0.01 BPM instead of 1 BPM
  (`TransportImpl.changeTempo`, `TransportImpl.java:704-708`). The faders and other encoders are
  slow as well.
- **Reproduce:** Maschine JAM. Press Shift + PAGE LEFT (metronome), release both. Hold TEMPO and
  turn the encoder: the tempo changes by 0.01 BPM. Press and release Shift alone: it is 1 BPM
  again.
- **Evidence:** `rust/tests/conformance/maschine_jam/CONFORMANCE.md` ("Possible Java bugs": the
  scenario used Shift + Page Left/Right first). The manual
  (`Native-Instruments/Native-Instruments-Maschine-Jam.md:64` and `:103`): "Use the encoder in
  combination with SHIFT for slower changes", so the normal step is the fast one. The manual does
  not give a number. 1 BPM is the normal step of `TransportImpl.changeTempo`.
- **Note:** the earlier guess was a wrong tempo step. The step is right; the slow flag is stale.
- **Fix:** clear the flag on every release, also a consumed one. `addEventHandler` runs for every
  UP (`AbstractHwButton.java:116`). In the setup, after the Shift button is added
  (`MaschineJamControllerSetup.java:282`):

  ```diff
           this.addButton (ButtonID.SHIFT, "SHIFT", new ToggleShiftViewCommand<> (this.model, surface), -1);
  +        // A consumed Shift does not pass its release to the command
  +        surface.getButton (ButtonID.SHIFT).addEventHandler (ButtonEvent.UP, event -> surface.setKnobSensitivityIsSlow (false));
  ```

  The other users of `ToggleShiftViewCommand` (APC, APCmini, Fire, Launchpad) consume Shift too.
  They may need the same line. A fix in `AbstractHwButton` would change all buttons, so it is not
  proposed.
- **Confidence:** high for the mechanism (code). A bug.
- **Ready:** needs a hardware check on the JAM. The Rust port copies the behaviour, so it needs
  the same fix (and a note in BITWIG_DIVERGENCES.md if Java does not take it).

#### D. The velocity spread of a step is never read from Bitwig

- **Where:** `src/main/java/de/mossgrabers/bitwig/framework/daw/StepInfoImpl.java:48-91`
  (`updateData`), and `CursorClipImpl.java:1231-1247` (`sendClipData`).
- **What is wrong:** `updateData` copies all attributes of `NoteStep` except `velocitySpread ()`
  (API 14, the project uses API 21). The local value stays 0. In a note edit
  (`startEdit` / `stopEdit`), `sendClipData` writes all attributes back, so it sets the spread
  to 0.
- **Reproduce:** set a note's velocity spread in Bitwig's inspector. On Push 2 (note edit), the
  spread shows 0. Hold the step, change the velocity, release: the spread in Bitwig is 0.
- **Evidence:** `BITWIG_CORE_PORT_ANALYSIS.md` (CursorClip, finding 5). The Rust extension reports
  the real value.
- **Fix:**

  ```diff
           this.velocity = stepInfo.velocity ();
  +        this.velocitySpread = stepInfo.velocitySpread ();
           this.releaseVelocity = stepInfo.releaseVelocity ();
  ```

- **Confidence:** high. A bug.
- **Ready:** yes, after a short check in Bitwig.

#### E. The step gain may be halved on every edit (checked: not a bug)

- **Result (2026-10-31):** checked on Bitwig with the Java Mk2. A 0 dB note shows as 50 %, a
  knob step up gives 60 % = +5 dB, 40 % is -14 dB. `gain ()` returns twice the scale of
  `setGain` (0.5 = 0 dB), so the `/ 2.0` makes them match. The javadoc is out of date. No PR.


- **Where:** `StepInfoImpl.java:72` reads `stepInfo.gain () / 2.0`.
  `CursorClipImpl.java:704-711` (`updateStepGain`) and `:1247` (`sendClipData`) write
  `setGain (g)` without the factor 2. `changeStepGain` (`:694-699`) adds the knob delta to the
  halved value. `setStep` (`:454`) copies the halved value.
- **What is wrong (if the scales are equal):** each edit of the gain, and each edit of any
  attribute in a note edit, halves the gain.
- **What speaks against it:** the `/ 2.0` came with upstream 86e5b994 (2024-01-02): "Fixed: The
  range of the gain note parameter seems to have changed. Adapted now." So Bitwig's `gain ()` may
  return 0..2 while `setGain` takes 0..1. The local javadoc
  (`/opt/bitwig-studio/resources/doc/control-surface/api/…/NoteStep.html`) says both are 0..1,
  and `setGain (0.5)` is 0 dB. The javadoc may be out of date.
- **Reproduce:** set a note to 0 dB in Bitwig. Turn the gain knob by one step on Push or Maschine
  (edit note mode). If the gain falls to about -6 dB or less, the bug is real.
- **Evidence:** `BITWIG_CORE_PORT_ANALYSIS.md` (CursorClip, finding 1), `BITWIG_DIVERGENCES.md` §6
  ("Step attributes"). The port copies the Java behaviour.
- **Fix:** depends on the check. Either read `gain ()` unscaled, or write `setGain (g * 2.0)` in
  `updateStepGain` and `sendClipData`.
- **Confidence:** low to medium. Unclear whether bug or correct.
- **Ready:** needs a check in Bitwig first (log the raw `gain ()` of a 0 dB note).

#### F. `getNextNote` / `getPreviousNote` return the wrong channel

- **Where:** `src/main/java/de/mossgrabers/bitwig/framework/daw/CursorClipImpl.java:1183` and
  `:1209`.
- **What is wrong:** with `ignoreChannel`, the loop searches all 16 channels but returns
  `new NotePosition (channel, …)`, the channel of the start position, not `chn`.
- **Reproduce:** Push clip mode (`ClipMode.java:261-263`, the only caller, with
  `ignoreChannel = true`). A clip with notes on two MIDI channels. Step to the next note: a note
  on the other channel is selected with the wrong channel, so editing it does nothing.
- **Evidence:** `BITWIG_CORE_PORT_ANALYSIS.md` (CursorClip, finding 6).
- **Fix:**

  ```diff
  -                        return new NotePosition (channel, step, row);
  +                        return new NotePosition (chn, step, row);
  ```

  (twice).
- **Confidence:** high for the code. A bug, but rare (multi-channel clips).
- **Ready:** after a check on Push.

#### G. Launch Control XL: the sequencer edit modes are deactivated twice

- **Where:** `src/main/java/de/mossgrabers/controller/novation/launchcontrol/LaunchControlXLControllerSetup.java:166-169`
  and `:192-193` register the same `drumPadEditMode` and `noteEditMode` in the main and the fader
  mode manager. `XLAbstractMainMode.java:209-214` (`onDeactivate`) stores the track button mode.
  `activateTemplate` (`:489-491`) switches both managers.
- **What is wrong:** on a template change, the mode is deactivated twice. The second
  `onDeactivate` runs after the new main mode set its own track button mode, and stores that one.
  Back in the sequencer, the Track Control row shows and toggles the track mutes (or record arm),
  not the step mutes.
- **Reproduce:** factory template 5 (Note Sequencer). Switch to template 1 (Mixer), then back to
  5. The Track Control row now toggles track mutes. Manual
  (`Novation/Novation-LaunchControlXL.md`, Note Sequencer): the buttons toggle the mute state of
  the note of the step.
- **Evidence:** `BITWIG_DIVERGENCES.md` §8, `rust/tests/conformance/launchcontrol_xl/CONFORMANCE.md`
  (known differences and possible Java bugs), commit 566e4a9b. Found by the conformance runner.
- **Fix:** store the mode only on the first deactivation. `isActive` is false after it
  (`AbstractMode.java:66`):

  ```diff
       public void onDeactivate ()
       {
  -        this.trackMode = this.surface.getTrackButtonModeManager ().getActiveID ();
  +        // The sequencer modes are also fader modes and are deactivated twice
  +        if (this.isActive)
  +            this.trackMode = this.surface.getTrackButtonModeManager ().getActiveID ();
  
           super.onDeactivate ();
       }
  ```

  Or register separate objects as fader modes, as the port does.
- **Confidence:** high. A bug.
- **Ready:** the conformance runner reproduced it on Bitwig. A check on the device is still
  good.

#### H. Launch Control XL: the Equalizer does not add an EQ+ on activation

- **Where:** `src/main/java/de/mossgrabers/controller/novation/launchcontrol/mode/main/XLEqMode.java:153-158`.
- **What is wrong:** `onActivate` calls `trackSelectionChanged (-1, true)` before
  `super.onActivate ()`. `trackSelectionChanged` returns early while `isActive` is false
  (`:169`), so no EQ+ is added.
- **Reproduce:** select a track without an EQ+. Switch to factory template 2. No EQ+ appears. It
  appears only after another track selection. Manual (`Novation-LaunchControlXL.md:71`): "If the
  track does not contain such a device it is automatically added."
- **Evidence:** `launchcontrol_xl/CONFORMANCE.md` (possible Java bugs, fixed by the comparison),
  commit 0b69c81c.
- **Fix:**

  ```diff
       public void onActivate ()
       {
  -        this.trackSelectionChanged (-1, true);
  -
           super.onActivate ();
  +
  +        this.trackSelectionChanged (-1, true);
       }
  ```

- **Confidence:** high. A bug.
- **Ready:** yes (runner result); a device check is cheap.

#### I. Launch Control XL: exception for a Note Sequencer step without a note

- **Where:** `src/main/java/de/mossgrabers/controller/novation/launchcontrol/mode/buttons/XLNoteSequencerMode.java:70-72`
  (`onButton`) and `:92-94` (`getSequencerStepColor`).
- **What is wrong:** `getHighestRow` returns -1 for an empty step. The code passes it as the row
  to `clip.getStep`. Once the step had a note, `CursorClipImpl.getStep` (`:407-421`) indexes the
  array with -1 and logs "Illegal index accessing step info array" with an
  `ArrayIndexOutOfBoundsException`, on every LED update. Bitwig's log of 2026-10-30 had about
  240000 of them. The LED and the button behave as intended. `XLBaseNoteEditMode.java:121` already
  checks for a row < 0.
- **Reproduce:** template 5. Create a note on a step, delete it. Bitwig's log fills with the
  exception.
- **Evidence:** `launchcontrol_xl/CONFORMANCE.md` (possible Java bugs), commit 4f41d6e6.
- **Fix:** d66bb744 on `conformance-osc`, without its `CONFORMANCE-OSC.md` hunk: return early in
  `onButton`, and use `EmptyStepInfo.INSTANCE` in `getSequencerStepColor` for a row < 0.
- **Confidence:** high. A bug.
- **Ready:** yes. Tested with the conformance runner.

#### J. Maschine Mk2 and JAM on Linux

- **Where:** `src/main/java/de/mossgrabers/controller/ni/maschine/mk3/MaschineControllerSetup.java:180-186`
  and `src/main/java/de/mossgrabers/controller/ni/maschine/jam/MaschineJamControllerSetup.java:150-156`
  throw a `FrameworkException` on Linux. `MaschineJamControllerDefinition.java:42-55` has no Linux
  discovery case.
- **What is wrong:** nothing, until there is a host for Linux. With dbm-nihost (Part 2) both
  devices work on Linux with Bitwig.
- **Reproduce:** Linux, Bitwig, dbm-nihost running. Add the Maschine Mk2 or JAM controller. Java
  refuses it with the exception.
- **Evidence:** `NI_HOST_LINUX_PLAN.md` (status, §5), `support/linux/ni_host/README.md`. Manual:
  `Native-Instruments-Maschine-Mk3.md:5` and `Native-Instruments-Maschine-Jam.md:5` say "only works
  on Windows and Mac (no Linux)".
- **Fix:** 219204b8 on `linux-maschine`: remove both `init` overrides, add a Linux case with NI's
  port names to the JAM discovery. The Mk2 default case already uses `Maschine MK2 In`/`Out`, the
  names of dbm-nihost.
- **Note:** on Bitwig, dbm-nihost goes through `snd-virmidi`, so the ports are named
  `Virtual Raw MIDI …` and discovery does not find them. The user selects them by hand.
- **Confidence:** high that it works (used with the Java version on Bitwig). An intended
  restriction, not a bug.
- **Ready:** after dbm-nihost is public. The PR should link it, and the manual needs a matching
  PR in DrivenByMoss-Documentation.

#### K. Chords which share notes: releasing one ends the shared notes

- **Where:** `src/main/java/de/mossgrabers/framework/view/AbstractChordView.java` (`onGridNote`,
  the JAM, Launchpad and Push chord views) and
  `src/main/java/de/mossgrabers/controller/ni/maschine/mk3/view/PlayView.java` (`onGridNote`, the
  chord mode of the Maschine Mk2/Mk3).
- **What is wrong:** the root of a chord is played by the note input of the pad. The added
  notes are sent with `sendRawMidiEvent`, and on the release each gets a note off. Nothing
  checks whether another held pad still plays the same note. So releasing chord A ends the notes
  it shares with the held chord B.
- **Reproduce:** Maschine JAM, Chords mode, C major. Hold the 1st pad of the lowest row (C E G),
  hold the 3rd pad (E G B), release the 1st: E and G stop, though the E triad is still held. The
  same in the chord mode of the Mk2 (PAD MODE). Reproduced by the user with the Java version
  on Bitwig (2026-10-31).
- **Fix:** keep the chords of the held pads. Send the note on of an added note only if no held
  chord plays it, and its note off only when no held chord plays it any more. The note input
  ends the root of a released chord in any case: if another held chord contains it, send its
  note on again. The Rust port does this in `HeldChords` (`rust/src/framework/view/play_view.rs`),
  test `chords_sharing_notes_keep_them_until_both_are_released` in
  `rust/tests/maschine_jam_setup.rs`.
- **Confidence:** high, a bug.
- **Ready:** yes; the same fix in the Rust port works on the JAM and Mk2 (2026-10-31).

#### L. Maschine Mk2/Studio: Shift + arrows (encoder up/down) also stops the transport

- **Where:** `src/main/java/de/mossgrabers/controller/ni/maschine/mk3/command/trigger/MaschineCursorCommand.java:61`
  and `:82` (`scrollLeft`, `scrollRight`).
- **What is wrong:** the Mk2 has no Shift in MIDI mode and no encoder up/down. STEP LEFT (the
  Stop button) is Shift, and Shift + the arrows below the encoder emulate up/down (the next or
  previous page of the mode, e.g. the Expressions page of the edit note mode). The command
  does not mark Stop as consumed, so releasing STEP LEFT afterwards still runs Stop: the
  transport stops, or the play position is set back to the start when it is not playing.
  Other Shift combinations do consume it, e.g. `PageCommand` (Shift + page left/right) and
  `MaschineTapMetroCommand`.
- **Reproduce:** Maschine Mk2 in MIDI mode. Play, press EVENTS and a pad with a note, hold STEP
  LEFT, press the right arrow (the next page), release STEP LEFT: the transport stops.
- **Evidence:** found while porting the edit note mode pages to the Rust port (2026-10-31),
  `rust/doc/BITWIG_DIVERGENCES.md` ("Maschine Mk2, page selection with STEP LEFT"). The Rust port
  copies the behaviour (test `edit_note_mode_has_3_pages_with_the_host_grid` in
  `rust/tests/maschine_mk2_setup.rs` asserts it). Manual: `Native-Instruments-Maschine-Mk3.md`,
  Mk2 section ("4D-Encoder up/down: Use the 2 arrows below the encoder with Shift (STEP LEFT)").
- **Fix:**

  ```java
  if (this.isUpDownEmulated && this.surface.isShiftPressed ())
  {
      this.surface.setStopConsumed ();
      super.scrollDown ();   // scrollUp () in scrollRight
      return;
  }
  ```

  On the Studio Shift is a real button; `setStopConsumed` there only marks Stop, which is not
  pressed, so it is harmless. The Rust port has this fix (`mk/command/trigger.rs`, test
  `edit_note_mode_has_3_pages_with_the_host_grid`), and the branch `conformance-osc` of the Java
  repository too.
- **Confidence:** high, a bug (the other Shift combinations consume Stop).
- **Ready:** yes; the fix works on the Mk2 in both versions (2026-11-01).

### 1.4 Checked, but not bugs (or not clearly)

| Topic | Java | Verdict |
| --- | --- | --- |
| XL Equalizer: band type knob with Bitwig's takeover mode catch (`XLEqMode`) | sets the EQ+ type parameter (16 values) directly; a jump is caught, "center is Bell" does not hold | maybe intended; a design difference |
| XL Note Sequencer: Device toggled on + Track Select pages the clip; only with Device held it selects the device (`XLBaseNoteEditMode`) | as the manual for the sequencer (`Novation-LaunchControlXL.md:123`) | not a bug; at most add "Device + Track Select" to the manual |
| XL Layer Mix with 2 layers: Track Select "previous" LED on | from Bitwig's `canScrollPageBackwards` | unclear; needs a look at the layer bank position |
| XL step knobs: CC / 128 | Bitwig's normalized hardware value | Bitwig binding; not a bug |
| `TransportImpl.changeTempo` slow step 0.01 BPM | as designed | the step is right; see C |

### 1.5 Keep apart

**Test scaffolding (not for upstream):** `ConformanceModule`, `/clip/step/…`,
`/track/N/playingNotes`, `/conformance/…`, the restart sysex in `MidiInputImpl`,
`CONFORMANCE-OSC.md`.

**Maybe, if upstream wants it:**

- The Maschine Mk2 button labels sysex `F0 7D 44 42 4D 10 <position> <56 characters> F7`
  (`BITWIG_DIVERGENCES.md` §6). Only the Rust port sends it. Java would need it in the Mk2 display
  code. NI's driver ignores it. `7D` is the non-commercial manufacturer ID, `44 42 4D` is "DBM".
  Useful only with dbm-nihost.
- OSC `/device/layer/N/browse`, `/device/drumpad/N/browse`, `…/addDevice {name}` (6957325b). Could
  be general OSC features. The `evenIfMissing` flag is test-specific and should go.
- `IApplication.getActionDescriptions` and an OSC action list (4fc63753). Could help OSC users find
  action IDs.

### 1.6 Proposed pull requests

One PR per topic. Each PR should be cut from `origin/master`, keep CRLF where the file has it, and
not contain test scaffolding.

| PR | Content | State |
| --- | --- | --- |
| 1. Launch Control XL: no exception for an empty Note Sequencer step | I | ready |
| 2. Launch Control XL: sequencer track buttons after a template change, EQ+ on activation | G, H | ready after a device check |
| 3. Bitwig: `setMonitor (false)` switches monitoring off | A | ready |
| 4. Bitwig: previous device at the first device of a page | B | after a Bitwig check |
| 5. Shift release after a Shift combination resets the slow knobs | C | after a JAM check |
| 6. Bitwig: note step velocity spread | D | ready (E, the gain, is not a bug) |
| 7. Push clip mode: channel of the next/previous note | F | after a Push check |
| 8. Maschine Mk2/JAM on Linux with dbm-nihost | J | after Part 2 |
| 9. Chords which share notes: keep them until all are released | K | ready (the Rust fix checked on the JAM and Mk2) |
| 10. Maschine Mk2: Shift + arrows does not also stop the transport | L | ready (`conformance-osc`) |

## Part 2: dbm-nihost as its own repository

### 2.1 What it is now

`rust/support/linux/ni_host`: 33 files, about 3700 lines. C++17, CMake, ALSA. It has:

- the HID protocol classes for the Mk2 and the JAM (`maschine_mk2.*`, `maschine_jam.*`) and the
  hidraw transport (`hidraw_device.*`), from KICK;
- the translators (`mk2_host.*`, `jam_host.*`), the palette, the MCU text display with a bitmap
  font, the ALSA sequencer ports and the daemon;
- `template_map.h`, generated from the NI templates;
- `nihost-probe` (tool), `nihost-test` (ctest with synthetic reports), `packaging/`.

A copy of the directory alone, outside this repository, builds and passes `ctest` (checked
2026-10-31 in `/tmp`). The C++ code has no include or path into the rest of the repository.

### 2.2 Dependencies on this repository

| Dependency | Where | How to cut it |
| --- | --- | --- |
| `tools/gen_template_map.py` reads `resources/Maschine Mk2/DrivenByMoss.ncm2` and `resources/Maschine Jam/DrivenByMoss.ncmj` from a "repository root" | the generator | Copy the two template files into the new repo (`templates/`), with their author and LGPL-3.0. Make the generator read them from there by default and take a path as option. The templates in this repo and in the Java repo are byte-identical, and the generator gives the same header from the Java repo (checked). |
| `src/template_map.h` | generated, committed | Keep it committed (vendored), so the build needs no Python. Add a CI step that regenerates it and fails on a difference. Add a header comment with the source and the license of the templates. |
| `rust/tests/ni_templates.rs` reads `support/linux/ni_host/src/template_map.h` | this repo | Keep the test here. It checks the Rust surface constants, not dbm-nihost. It keeps working if the new repo comes back to the same path (see 2.9). In the new repo, add a small C++ test of a few known numbers instead (Mk2 `Browse` = CC 40, JAM matrix notes). |
| `rust/ci/linux.sh` builds and tests it | this repo | The new repo gets its own CI (2.5). This repo keeps the step for the vendored copy. |
| `doc/NI_HOST_LINUX_PLAN.md` | this repo | Move the design part (§1–4, §6–8) into the new repo as `doc/DESIGN.md`. Keep a short note here that points to it. |
| systemd unit `Documentation=` | `packaging/dbm-nihost.service.in` | Point it to the new repository. |
| Naming: `DrivenByMoss` in the service description, `dbm::nihost` namespace | sources, packaging | Keep the `dbm-nihost` name (it is known now). Describe it as "NI Maschine Mk2/JAM MIDI host for Linux". The namespace can stay. |
| `target_include_directories(… src include)` | CMakeLists.txt | `include/` does not exist (the C API of plan §4.1 is not written). Remove it, or add `include/ni_host.h` when stage 2 comes. |
| README build commands use `support/linux/ni_host` paths | README | Change to the repo root (`cmake -S . -B build`). |

### 2.3 Licensing

- The own code is LGPL-3.0 (per-file headers). The KICK files were relicensed from AGPL-3.0 by the
  same author; the headers say so.
- The cabl parts are MIT, (C) 2015 Vincenzo Pacella: the protocol tables in `maschine_*.{h,cpp}`,
  `font.h` and part of `palette.cpp`. `LICENSE-cabl.txt` is present.
- What the new repo needs:
  - `COPYING.LESSER` (LGPL-3.0 text) and `COPYING` (GPL-3.0 text). The LGPL-3.0 is a set of
    additional permissions on the GPL-3.0, so both texts belong in the repo. Today the directory
    has neither; it relies on this repo's `LICENSE`.
  - `LICENSE-cabl.txt` as is.
  - A short `LICENSE` or README section that says which files are under which license.
  - A header in `template_map.h`: generated from the DrivenByMoss templates, (c) Jürgen Moßgraber,
    LGPL-3.0. The same for the copied template files.
  - SPDX lines would help (`SPDX-License-Identifier: LGPL-3.0-only` or `-or-later`, as decided;
    and `MIT` for `font.h`). The current headers do not say "or later". Decide once and use it
    everywhere.
- The `.py` generator is LGPL-3.0 like the rest.

### 2.4 Parts specific to DrivenByMoss

| Part | Decision |
| --- | --- |
| The MIDI mapping comes from the DrivenByMoss templates | Keep. It is the point of the tool: DrivenByMoss's NI templates are the "MIDI mode" it emulates. Document that other DAWs see this mapping. A later option could load another `.ncm2`/`.ncmj` at run time. |
| Label sysex `F0 7D 44 42 4D 10 <position> <chars> F7` (Mk2 button labels) | Keep and document as an extension of the protocol. It is passive: without it, the label row stays empty. Document the `F0 7D 44 42 4D` ("DBM") namespace and its message IDs: `10` labels, `03` is used by the conformance tests (restart, Java branch only). An option to turn it off is not needed. |
| Mk2 MCU text on the displays (`F0 00 00 66 14 12 …`) | Keep. It is the Mackie protocol, any DAW can send it. |
| JAM `STRIP_SETUP` and Shift sysex (`F0 00 21 09 15 00 4D 50 …`) | Keep. These are NI's messages. |
| `--virmidi` uses the first two `snd-virmidi` clients, Mk2 first | Keep, but add an option to choose the clients (the conformance tests use `midi_devs=4`). Document it as "for DAWs that only open raw MIDI devices (Bitwig)". |
| Port names as NI's driver (`Maschine MK2 In`, `Maschine Jam - 1 Input`) | Keep. Hosts with NI discovery pairs find them. |
| Plan §7a (real Shift, Page left/right on the Mk2) | Would only work with dbm-nihost. If it comes, make it an option that is off by default. |

### 2.5 Build, install, packaging, CI

- **Build:** CMake ≥ 3.15, C++17, `alsa` via pkg-config, Threads. Unchanged.
- **Install:** `cmake --install` puts `bin/dbm-nihost`, `share/systemd/user/dbm-nihost.service` and
  `share/dbm-nihost/` (udev rule, modprobe and modules-load files). Add:
  - an option `-D NIHOST_INSTALL_SYSTEM_FILES=ON` for packagers, which installs the udev rule to
    `lib/udev/rules.d` and the module files to `lib/modprobe.d` and `lib/modules-load.d` (for
    `/usr`);
  - `nihost-probe` as an optional install target.
- **udev:** `70-dbm-nihost.rules` with `TAG+="uaccess"` for `17cc:1140` and `17cc:1500`. Unchanged.
- **systemd:** the user service runs `dbm-nihost --virmidi` with `Restart=on-failure`. Fix the
  `Documentation=` URL.
- **snd-virmidi:** `options snd-virmidi midi_devs=2` and the modules-load file. Document that it is
  only needed for Bitwig.
- **Distribution packages (later):** an AUR `PKGBUILD` and a Debian `debian/` directory are easy
  with the install option above. Not needed for the first release.
- **CI:** a GitHub Actions workflow on `ubuntu-latest`: install `cmake g++ libasound2-dev`,
  configure, build, `ctest`, and the template check (regenerate `template_map.h`, `git diff
  --exit-code`). Optionally build with `clang` and `-Werror`. No hardware.
- **Version:** CMake says 0.1.0. Tag `v0.1.0` at the first release.

### 2.6 Git history

- Four commits touch `rust/support/linux/ni_host`: 9846121f, 295e73cf, 86581896, 0d3b271c. No merge
  commits. Two more touch only the plan doc: c6cfa77a, fcf049a8.
- Each of the four also changes files outside the directory (Rust code, docs, conformance data,
  the Rust test). The history of the directory itself is self-contained: no file moved in from
  outside, and the rename `udev/70-ni-maschine.rules` → `packaging/70-dbm-nihost.rules` is inside it.
- **`git subtree split --prefix=rust/support/linux/ni_host`** works as is. It gives four commits
  with only the directory. The messages keep the "Rust port:" prefix and mention other files. The
  plan doc is not carried over.
- **`git filter-repo`** (not installed here) can do more in one pass:
  `--path rust/support/linux/ni_host/ --path rust/doc/NI_HOST_LINUX_PLAN.md
  --path-rename rust/support/linux/ni_host/: --path-rename rust/doc/NI_HOST_LINUX_PLAN.md:doc/DESIGN.md`
  and a `--message-callback` that drops the "Rust port: " prefix. It must run on a fresh clone.
- **Recommendation:** filter-repo on a fresh clone, so the design doc keeps its history. Then add
  the license files, the README and CI as new commits. Or, as simpler: start the new repo with one
  import commit that names the source commit (0d3b271c), since the history is short.

### 2.7 Proposed layout

```
dbm-nihost/
  CMakeLists.txt
  README.md
  COPYING                  GPL-3.0 text
  COPYING.LESSER           LGPL-3.0 text
  LICENSE-cabl.txt         MIT, cabl
  doc/
    DESIGN.md              from NI_HOST_LINUX_PLAN.md §1–4, §6–8
    PROTOCOL.md            the MIDI per control, the sysex messages, the DBM sysex namespace
  src/                     as now
  include/                 later: ni_host.h, the C API for in-process use (plan §4.1)
  templates/
    DrivenByMoss.ncm2      copied from DrivenByMoss resources/, LGPL-3.0, (c) Jürgen Moßgraber
    DrivenByMoss.ncmj
  tools/
    gen_template_map.py    reads templates/ by default
    probe.cpp
  tests/
    host_test.cpp
    template_test.cpp      a few known numbers
  packaging/               as now
  .github/workflows/ci.yml
```

### 2.8 README outline

1. What it is: NI Maschine Mk2 and JAM on Linux, as MIDI ports, without NI's driver.
2. Supported devices and what works (input, LEDs, displays; not yet: pad pressure, JAM meters,
   footswitch).
3. Which software works with it: DrivenByMoss for Bitwig (Java), DrivenByMoss for Reaper (Rust
   port), any DAW that takes the MIDI of the DrivenByMoss templates.
4. Build.
5. Install: binary, udev rule, systemd user service, `snd-virmidi` for Bitwig.
6. Use: Reaper (PipeWire MIDI bridge names), Bitwig (`Virtual Raw MIDI`; do not open the Mk2's own
   MIDI port), other DAWs.
7. The MIDI mapping: the templates, the port names, the extension sysex (`F0 7D 44 42 4D 10`).
8. Troubleshooting: permissions, `--log-midi`, `nihost-probe`, the journal.
9. Development: tests, regenerating the template tables.
10. License and origin (KICK, cabl, DrivenByMoss templates).

### 2.9 The follow-up in this repository

Options:

- **Submodule:** clean, but every clone needs `--recursive`, and the Rust test and the CI break
  without it.
- **Vendored copy (squashed `git subtree`):** as `backends/reaper` is today (STATUS.md, "Squashed
  git subtree of upstream"). Same path, so `tests/ni_templates.rs` and `ci/linux.sh` keep working.
  Updates with `git subtree pull --prefix=rust/support/linux/ni_host <url> master --squash`.
- **External dependency:** the daemon is a separate program, so this repo could just document it.
  But then the Rust test loses its table, and stage 2 (in-process, plan §4.1) needs the sources.

**Recommendation:** squashed subtree at `rust/support/linux/ni_host`. Then:

- replace the README here with a short pointer to the new repo (or keep the subtree's README);
- move the design part of `NI_HOST_LINUX_PLAN.md` to the new repo, keep the plan's status and §5
  (changes outside) here;
- update STATUS.md (table of external parts) and the systemd URL;
- keep `tests/ni_templates.rs` and the `ci/linux.sh` step.

## Summary

| ID | Candidate | Where | Kind | Confidence | State |
| --- | --- | --- | --- | --- | --- |
| A | `setMonitor (false)` sets ON | `TrackImpl.java:261` | bug | high | ready |
| B | previous device at a page edge ends on item 0 | `CursorDeviceImpl.java:109` | bug | high | Bitwig check |
| C | consumed Shift leaves knobs slow (JAM tempo 0.01 BPM) | `AbstractHwButton.java:113`, `ToggleShiftViewCommand.java:70` | bug | high | JAM check |
| D | velocity spread never read, reset by note edits | `StepInfoImpl.java:66` | bug | high | ready (short check) |
| E | step gain halved on edit | `StepInfoImpl.java:72`, `CursorClipImpl.java:704` | not a bug (checked on Bitwig) | – | no PR |
| F | next/previous note returns the start channel | `CursorClipImpl.java:1183, 1209` | bug | high | Push check |
| G | XL sequencer modes deactivated twice | `LaunchControlXLControllerSetup.java:168, 193`, `XLAbstractMainMode.java:211` | bug | high | device check |
| H | XL EQ+ not added on activation | `XLEqMode.java:155` | bug | high | ready |
| I | XL exception for an empty step | `XLNoteSequencerMode.java:70, 92` | bug | high | ready (d66bb744) |
| J | Maschine Mk2/JAM refuse Linux | `MaschineControllerSetup.java:180`, `MaschineJamControllerSetup.java:150` | intended restriction | high | after dbm-nihost (219204b8) |
| K | releasing a chord ends the notes shared with a held chord | `AbstractChordView.java`, Maschine `PlayView.java` | bug | high | ready |
| L | Shift (STEP LEFT) + arrows also runs Stop on release | `MaschineCursorCommand.java:61, 82` | bug | high | ready |
| – | XL EQ type knob with catch, Device + Track Select, Layer Mix LED | `XLEqMode`, `XLBaseNoteEditMode` | design / unclear | – | not for a PR |
| – | Mk2 label sysex, OSC browse/addDevice, action list | Rust port, `conformance-osc` | port-only additions | – | maybe, if upstream wants it |
| – | OSC conformance module, restart sysex | `conformance-osc` | test scaffolding | – | not for upstream |
| P2 | dbm-nihost as its own repo | `rust/support/linux/ni_host` | new repo | high (builds alone) | needs license files, template copies, CI, README; then a squashed subtree here |
