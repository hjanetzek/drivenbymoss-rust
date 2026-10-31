# Ported Bitwig host functions without a conformance scenario

The host functions ported from the Java DrivenByMoss for Bitwig (BITWIG_CORE_PORT_PLAN.md,
BITWIG_DIVERGENCES.md §6, HOST_BRIDGE_GUIDE.md, STATUS.md "Host bridge findings", and the working
tree of 2026-10-31: cue markers, crossfader, time signature, pre-roll, action, track/device moves,
track remote controls) were checked with the conformance scenarios of the Launch Control XL,
Maschine JAM and Maschine Mk2 (`tests/conformance/*/*.toml`). This list contains the functions which
no scenario exercises. For each one it gives the ported Rust controllers which call it, the Java
controllers which use it (also the ones not ported yet), how to trigger it on the user's hardware,
and what a scenario would have to observe.

**Already covered by scenarios (left out):** track select/mute/solo/rec arm, clear all mutes/solos
(`project unmuteAll|unsoloAll`: `jam.transport.clear-*`, `mk2.global.clear-*`,
`lcxl.mix.transport-defeat`), punch in (`jam.transport.punch-in`), metronome, transport loop,
arranger/launcher automation write, arranger/launcher overdub, tempo by encoder, note repeat,
device page selection, the EQ type, layer and drum pad select/mute, step on/off and step mute,
step velocity, chance, repeat count and panorama (LCXL sequencers; chance and velocity only
compared between the versions, no expectation), clearing a drum pad's notes (`jam.drum.clear-pad`),
clip launch/select/stop, the Mk2 mode buttons (pressed only, no knob turned).

**Observers for scenarios.** "OSC" is the OSC controller of the Java DrivenByMoss
(`controller/osc/module/*Module.java`, `expect.bitwig` without prefix). "probe" is the probe of
the Rust extension (`ProbeExtension.java`, `probe:` prefix): it reports the protocol of
`BitwigModel` (HOST_BRIDGE_GUIDE.md), e.g. `/groove/*`, `/project/cueVolume`, `/fill`,
`/clip/loopStart`, `/beats/loop/start`; its fixed model setup has no markers. Scenarios which
change the project (add track, new clip) need `--fresh-project` or an undo in `before`.

**The user's hardware.** Launch Control XL (LCXL), Maschine JAM, Maschine Mk2 (Shift is STEP LEFT,
STEP RIGHT is TAP/METRO, PAGE LEFT/RIGHT are not sent in MIDI mode), possibly a Launchpad (all 5
models are ported: `controller/launchpad/`, "LP" below; names of the Pro model as in the manual).

## 1. Transport

| Feature | Rust controllers and trigger | Java controllers | Manual test with the user's hardware | Scenario possible? |
| --- | --- | --- | --- | --- |
| Punch out (`transport punchOut`) | `framework/commands/transport.rs:PlayCommand::execute_shifted` (Select held): JAM SELECT+SHIFT+PLAY; Mk2 SELECT+STEP LEFT+PLAY (code path, not in the Mk2 manual) | all users of `PlayCommand` (Push, APC, Fire, MCU, Kontrol, Maschine, LCXL, Launchkey, Launchpad, …); HUI `PunchOutCommand`, MCU assignable buttons | JAM: hold SELECT and SHIFT, press PLAY: Bitwig's punch-out button toggles | yes, OSC `/punchOut` (as `jam.transport.punch-in`) |
| Metronome ticks (`/click/ticks`) | `framework/commands/transport.rs:MetronomeCommand::execute_shifted` with `tap_with_shift = false`: only the Maschine Studio METRO button (`mk/setup.rs:register_maschine_studio_buttons`); the Studio is not registered (`maschine/mk/mod.rs:SUPPORTED_MODELS = [Mk2]`). Mk2 Shift+TAP calls only `execute_normal` (`mk/command/trigger.rs:MaschineTapMetroCommand`) | Fire, Kontrol Mk1, MCU, OXI One, Maschine Studio | none on the user's hardware | OSC `/click/ticks`, once a controller uses it |
| Metronome volume (`transport metronome volume`, ParamRef::MetronomeVolume) | JAM encoder mode IN 1 (`jam/setup.rs:517-520`, `core/encoder_mode_manager.rs:change_target`); LCXL fader 7 while a Track Focus button is held (`launchcontrol/mode/main.rs:handle_row0` → `framework/mode/master.rs:MasterAndFXVolumeMode`) | Push, Maschine (JAM, Studio SND), MCU, SL MkIII, Electra One, OXI, Flexi | JAM: press IN 1, turn the encoder: Bitwig's metronome volume. LCXL: hold Track Focus 1, move fader 7 | yes, OSC `/click/volume`, `/click/volumeStr` |
| Fill mode (`/fill`) | `maschine/core/command/note_repeat.rs:FillModeNoteRepeatCommand::execute` (Select held): JAM and Mk2 SELECT+NOTE REPEAT | Push, Maschine, OXI One | JAM/Mk2: hold SELECT, press NOTE REPEAT: Bitwig's Fill button toggles, the display/notification shows "Fill Mode: On" | yes, probe `/fill` (OSC does not report it) |
| Arranger play position, loop start, loop length (`time loop/start|length` in beats, newly handled) | JAM encoder mode GRP (play position), encoder press: loop start, SELECT held: loop length (`core/encoder_mode_manager.rs:change_target`); Mk2 SWING once/twice/three times: Position, Loop Start, Loop Length (`mk/command/trigger.rs:SwingCommand`, `mk/mode/transport_modes.rs`) | Maschine (JAM, Mk3 `LoopStartMode`/`LoopLengthMode`), APC40 (play position), Push | Mk2: press SWING twice, turn the encoder: the arranger loop start moves; three times: the loop length | yes, probe `/beats/loop/start`, `/beats/loop/length`, `/beats/position`; OSC `/beat/str` for the position |
| Pre-roll measures, pre-roll metronome (`inifile reaper/prerollmeas`, `/click/prerollMeasures`) | none (`transport.rs:set_preroll_measures`, `toggle_preroll_metronome` have no caller in `controller/` or `framework/`) | Push `MetronomeMode`, Flexi | – | OSC `/preroll`, `/click/preroll` |
| Time signature (`time signature`) | none (`transport.rs:set_time_signature` has no caller) | none (only the OSC module's `/time/signature` command) | – | OSC `/time/signature` |
| Crossfader (`transport crossfade`, ParamRef::Crossfader) | none: `ParamRef::Crossfader` is only resolved in `framework/parameterprovider/mod.rs`, never constructed; `Modes::Crossfader` is only listed in the track mode tables | Push `CrossfadeMode`, touchstrip, APC40 crossfader, ACVS (MPC), BeatStep, SL touchpad, Yaeltex Turn, EC4, Flexi | – | OSC `/crossfade` |
| Groove: enabled | JAM SELECT+SWING (`jam/command/mod.rs:MaschineJamSwingCommand::execute_normal` → `toggle_groove_parameter`); LP shuffle view pad 36 (`launchpad/view/tempo_view.rs:LaunchpadShuffleView::on_grid_note`) | Push `GrooveMode`, Maschine, Launchpad, MCU, Electra, OXI, Flexi | JAM: hold SELECT, press SWING: Bitwig's groove switch toggles, the display shows the value | yes, probe `/groove/enabled` |
| Groove: shuffle amount | JAM SWING held + encoder (`EncoderMode::TemporarySwing`, `encoder_mode_manager.rs`); LP Shift+Left/Right then the arrows (`launchpad/command/cursor.rs:379`) | as above | JAM: hold SWING, turn the encoder | yes, probe `/groove/shuffleAmount` |
| Groove: shuffle rate | JAM SHIFT+SWING (`MaschineJamSwingCommand::execute_shifted`); LP shuffle view pads 38/39 | Push, Maschine, Launchpad, OXI | JAM: SHIFT+SWING toggles 1/8 and 1/16 | yes, probe `/groove/shuffleRate` |
| Groove: accent amount, rate, phase | none (no `GrooveParameterId::Accent*` in `controller/`) | Push `GrooveMode`, OXI One `OxiOneGrooveMode`, Flexi | – | probe `/groove/accent*` |

## 2. Cue markers

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| Add, launch, select, remove, previous/next (`marker add`, `marker <i>/launch|select|remove`, `jumpToPrevious/NextCueMarker`) | none (no `marker_bank` in `controller/` or `framework/`; tested only by `parser.rs:host_markers`) | Push `MarkerMode`, MCU `MarkerMode` (+ marker navigation in `MCUWindCommand`, `SelectCommand`, `MCUMoveTrackBankCommand`), APC40 (`APCCursorCommand`, `APCTapTempoCommand`), Electra One `TransportMode`, Flexi | – | yes, OSC `/marker/<n>/exists|name|color` (Java `MarkerModule`); the probe reports no markers (no `numMarkers` in its model setup) |

## 3. Application and project

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| Panel layout (`/layout`, `application layout`, LayoutStep) | JAM SHIFT+SONG arrange/mix (`jam/command/mod.rs:MaschineJamSessionViewCommand::execute`); Mk2 ARRANGER next layout, Shift+ARRANGER Edit (`framework/commands/misc.rs:LayoutCommand`), Shift+PLUG-IN next layout (`PanelLayoutCommand::execute_shifted`); LP Pro Mk3 project view pads 54/56/58 (`launchpad/view/project_view.rs`) | Push, ACVS, APC, MCU, HUI, Maschine, Launchpad, SL | JAM: SHIFT+SONG switches Arrange/Mix. Mk2: ARRANGER steps through the layouts | yes, OSC `/layout` |
| Panes (mixer, devices, note/automation editor) | Mk2 MIXER toggles the mixer, Shift+MIXER the device panel (`misc.rs:PaneCommand`, `mk/setup.rs:844`); LP Pro Mk3 project view | APC, MCU, HUI, Maschine, Launchpad | Mk2: MIXER shows/hides Bitwig's mixer pane | no (the OSC controller does not report the panes) |
| Engine on/off (`project engine`) | LP Pro Mk3 project view pads 95/96 (`project_view.rs`) only | Push/MCU `MasterMode`, HUI, SL MkIII `OptionsMode`, Launchpad `ProjectView` | only with a Launchpad Pro Mk3 | yes, OSC `/project/engine` (the runner already sends `/project/engine 1`) |
| Action invoke (`action <id>`) | none (`application.rs:invoke_action` has no caller) | MCU assignable buttons, EC4, Electra One (touch combinations), Flexi | – | depends on the action |
| Add track (`track addTrack`) | JAM SELECT+MACRO/LEVEL/AUX: instrument/audio/effect (`jam/command/mod.rs:MaschineJamMacroCommand`, `…LevelCommand`, `…AuxCommand::execute_normal`); Mk2 CONTROL: instrument, with STEP LEFT (Stop) held: audio (`misc.rs:AddTrackCommand::execute`, `mk/setup.rs:845-850`); LP Shift view pads 97/98/99 (`launchpad/view/shift_view.rs:325`) | Push, APC, Fire, BeatStep, HUI, Maschine, Launchkey, Launchpad, SL MkIII, Flexi | JAM: hold SELECT, press MACRO: a new instrument track at the end, which gets selected | yes, OSC `/track/<n>/exists|type` (with `--fresh-project`) |
| Cue volume, cue mix (`project cueVolume|cueMix`) | JAM HEADPHONE: cue volume, encoder press: cue mix (`jam/setup.rs:520`, `encoder_mode_manager.rs:change_target`) | Maschine (JAM, Studio CUE/IN4), Push `MasterMode`, Electra One, Yaeltex Turn, EC4 | JAM: press HEADPHONE, turn the encoder; press the encoder, turn again (cue mix) | yes, probe `/project/cueVolume`, `/project/cueMix` (OSC does not report them) |
| Project remote controls (`project param/…`) | JAM CONTROL (User mode, project first), SELECT+CONTROL toggles project/track (`jam/mode/mod.rs:MaschineJamUserMode`, `toggle_user_mode`); Mk2 User Params (`mk/mode/user_mode.rs:MaschineUserMode`, project by default); LCXL Track Mix: Device held + Send Select steps device → track → project parameters (`launchcontrol/mode/main.rs:TRACK_MIX_BANKS`, `select_parameter_mode_of_track_mix`); LP Device twice (`launchpad/view/user_view.rs`) | Push, MCU, Maschine, LCXL, Launchpad, Launchkey, SL MkIII, Kontrol, Fire, … | JAM: press CONTROL until the strips are white, move a strip: a project remote control in Bitwig moves | yes, OSC `/project/param/<n>/value`, `/project/page/<n>/selected` (needs project remote controls in the fixture) |
| Record quantization (`/recordQuantization`, global in Bitwig) | JAM SHIFT+GRID next, SELECT+GRID previous (`jam/command/mod.rs:MaschineJamQuantizeCommand`, `framework/commands/clip.rs:QuantizeCommand::select_record_quantization`); LP Shift+Quantise | Push `QuantizeMode`, Fire `PlaySelectCommand`, and the `QuantizeCommand` users Kontrol, Launchkey, Launchpad | JAM: SHIFT+GRID several times: Bitwig's record quantization grid changes (display shows it) | yes, OSC `/track/<n>/recordQuantization` (or probe `/recordQuantization`) |
| Browser (`BitwigBrowser`) | JAM BROWSE, encoder, pads (`jam/command/mod.rs:MaschineJamBrowserCommand`, `encoder_mode_manager.rs:handle_browser`); Mk2 BROWSE, Shift/Select+BROWSE, SETTINGS (`mk/setup.rs:684`, `mk/command/trigger.rs:AddDeviceCommand`); LP Device twice / Shift+Device, device view scene 8 (`launchpad/view/browser_view.rs`, `device_view.rs`); session BROWSE+pad (`framework/view/session_view.rs`), drum pad browse (`abstract_drum_view.rs`) | Push, APC, APC mini, Fire, BeatStep, MCU, Kontrol, Maschine, Launchkey, Launchpad, SL, SL MkIII, Flexi | JAM: BROWSE opens Bitwig's browser on the cursor device, the encoder moves the result, encoder press confirms | yes, OSC `/browser/isActive`, `/browser/tab`, `/browser/filter/<n>/item/<m>/…` |

## 4. Tracks

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| Track remote controls (`track <p>/param/…`, `/track/fx/param/…` of the cursor/pinned track) | JAM SELECT+CONTROL from project to track parameters (amber strips); LCXL Device held + Send Select (second step); LP user view cursor up/down; Mk2: only via PAGE LEFT/RIGHT (`mk/command/trigger.rs:PageCommand::execute`), which the Mk2 does not send in MIDI mode, so not reachable on the Mk2 | as project remote controls | JAM: CONTROL, then SELECT+CONTROL (strips amber), move a strip: a track remote control of the selected track moves. Known gap: no page names, the page does not follow Bitwig (STATUS.md) | yes, OSC `/track/param/<n>/value`, `/track/page/<n>/selected` (needs track remote controls on the fixture track) |
| Pinned track (`track pinned/toggle`) | Mk2 Pin (button 5 above the display) outside the Parameters mode (`mk/setup.rs:register_display_buttons`, `model.rs:toggle_cursor_track_pinned`); LP device view scene 5 pins device and track (`launchpad/view/device_view.rs:on_button`) | Push, ACVS, APC, Fire, MCU, Maschine, Launchkey, Launchpad, SL MkIII, Electra, Flexi | Mk2: Volume mode, press Pin, select another track in Bitwig: the controller stays on the pinned track | yes, OSC `/track/selected/pinned` |
| Track activation (`isActivated`) | Mk2 Active (button 6) outside the Parameters mode (`mk/setup.rs:905`) | Maschine, Push, MCU, … | Mk2: Volume mode, Active: the selected track is deactivated | yes, OSC `/track/<n>/activated` |
| Track move (`track <p>/movePrev|moveNext`) | none (`track.rs:swap_with_previous/next` have no caller) | Push track modes (`AbstractTrackMode`), MCU `MCUMoveTrackBankCommand` | – | yes, OSC `/track/<n>/name` |

## 5. Devices

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| Cursor device window (`device window/toggle`) | JAM SHIFT+CONTROL (`MaschineJamControlCommand::execute_shifted`); Mk2 PLUG-IN (`misc.rs:PanelLayoutCommand::execute_normal`, `mk/setup.rs:610`); LCXL Rec Arm held + Track Focus 8 (`launchcontrol/mode/main.rs:handle_transport`); LP device view scene 2 | Push, APC mini, Fire, BeatStep, Electra, EC4, Kontrol, Maschine, LCXL, Launchkey, Launchpad, SL, SL MkIII, OXI, Flexi | LCXL: hold Rec Arm, press Track Focus 8: the plug-in window opens (needs a plug-in device; Bitwig devices have no window) | yes, OSC `/device/window` (fixture with a plug-in) |
| Cursor device expand (`device expand/toggle`) | Mk2 SELECT+PLUG-IN (`PanelLayoutCommand::execute`); LP device view scene 4 | Push, Electra, Launchpad, SL MkIII, Flexi | Mk2: hold SELECT, press PLUG-IN: the device collapses/expands in Bitwig | yes, OSC `/device/expand` |
| Cursor device bypass (`device bypass`) | Mk2 Active in the Parameters mode (`mk/setup.rs:903`); LP device view scene 1 | Push, ACVS, Fire, BeatStep, Electra, MCU, Kontrol, Maschine, Launchkey, Launchpad, SL, SL MkIII, Flexi | Mk2: Parameters, then Active: the device is switched off | yes, OSC `/device/bypass` |
| Pinned device (`device pinned`) | Mk2 Pin in the Parameters mode (`mk/setup.rs:885`); LP device view scene 5 | as pinned track | Mk2: Parameters, Pin, select another device in Bitwig: the controller stays | yes, OSC `/device/pinned` |
| Remote controls section (LP scene 3) | `device.rs:toggle_parameter_page_section_visible` is a no-op, LP scene 3 does nothing on Bitwig | Launchpad, Push | – | OSC `/device/parameters` (after porting) |
| Cursor device remove / duplicate (`device remove|duplicate`) | none | Fire `FireSelectButtonCommand` (remove); duplicate: none found | – | OSC `/device/sibling/<n>/name` |
| First instrument / drum / EQ commands without index (`primary|drum|eq window/toggle|expand/toggle|bypass|remove|duplicate`) | none (only the cursor device's are called) | Push and others via `getFirstInstrument` | – | OSC `/primary/window|bypass|expand`, `/eq/…` |
| Device siblings paging (`device_bank().select_previous/next_page`, `sibling/<i>/…`) | none reachable: `framework/mode/device.rs:ParameterMode::select_previous/next` pages siblings with the alternative function, but JAM's `MaschineJamParameterMode` does not forward `select_previous_item` and the Mk2 `parameters_mode.rs` selects devices directly | Push, ACVS, Electra, EC4, Exquis, MCU, Kontrol, SL MkIII, Yaeltex | – (selecting the previous/next device is a different path: LCXL Device held + Track Select, `main.rs:handle_page_combination`; Mk2 NAVIGATE pads 13/14) | OSC `/device/sibling/<n>/selected` |
| Device move (`device <i>/movePrev|moveNext`) | none (`device.rs:swap_with_previous/next` have no caller) | Push `DeviceParamsMode` | – | OSC `/device/sibling/<n>/name` |

## 6. Parameters

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| Relative inc (`<value>/inc`, valueInc) | relative encoders only: Mk2 knobs 1-8 in Volume/Pan/Send/Parameters/User Params, the 4D encoder (`mk/setup.rs:register_continuous_commands`, `ContinuousKind::RelativeKnob`); JAM encoder in MST (master volume/pan) and HEADPHONE (`encoder_mode_manager.rs:change_target`; IN 1 is the separate `MetronomeVolumeParameter`). The LCXL knobs, JAM strips and LP faders are absolute. The tempo (covered) is not a `Parameter` | all controllers with relative encoders (Push, MCU, Maschine, Kontrol, SL MkIII, Fire, EC4, …) | Mk2: Volume mode, turn knob 1 slowly: the volume of track 1 moves in small steps also when the takeover mode is "catch" | yes, OSC `/track/<n>/volume`, `/master/volume`, `/device/param/<n>/value` |
| Value reset (`<value>/reset`, valueReset) | Erase held + touch a knob (`framework/mode/track.rs:on_knob_touch`, `framework/mode/device.rs`, `parameter_mode.rs:613`): Mk2 Volume/Pan/Send/Parameters modes. Needs knob touch messages (CC 60-67, `mk/surface.rs:MODE_KNOB_TOUCH_1`); verify that the Mk2 template sends them. No reset on JAM, LCXL or LP | Push, ACVS, Fire, Electra, EC4, MCU, HUI, Maschine, OXI, Flexi | Mk2: hold ERASE, touch knob 1 in Volume mode: the volume jumps to Bitwig's default (0 dB) | yes, OSC `/track/<n>/volume` after the touch (the runner sends CCs, also the touch CC) |
| Indications (`indicate`, `setShouldShowClipLauncherFeedback`) | track bank at startup: LCXL, JAM, Mk2 (`launchcontrol/setup.rs:89`, `jam/setup.rs:72`, `mk/setup.rs:144`); drum pads in the drum views (`framework/view/sequencer/abstract_drum_view.rs:181`, `drum64_view.rs:92`); LP per mode: volume, pan, sends, device parameters (`launchpad/setup.rs:update_indication`) | nearly all (Push, APC, Fire, MCU, Maschine, LCXL, Launchkey, Launchpad, …) | look at Bitwig: the colored frame around the 8 tracks × scenes in the launcher; JAM drum sequencer: the drum pads of the page are marked | no (only visible in Bitwig's GUI) |

## 7. Clips and steps

| Feature | Rust controllers and trigger | Java controllers | Manual test | Scenario possible? |
| --- | --- | --- | --- | --- |
| New clip (`create_note_clip`) | LCXL Rec Arm held + Track Focus 1 (`main.rs:handle_transport` → `framework/commands/clip.rs:NewCommand::handle_execute`, without overdub); Shift+REC with the default setting "New clip" (`framework/commands/transport.rs:ConfiguredRecordCommand`, `config/configuration.rs:212`): JAM, Mk2, LP; Mk2 MACRO (`mk/setup.rs:543`); LP Shift+Double (`launchpad/command/misc.rs:PlayAndNewCommand`); session pad on a rec-armed track with "Action for pressing rec armed empty clip" = create (`framework/view/session_view.rs:handle_recording`); JAM footswitch "Clip Based Looper" | Maschine `ClipView`, and the `NewCommand` users (Push, APC, Fire, MCU, LCXL, Launchkey, Launchpad, SL, …) | LCXL: hold Rec Arm, press Track Focus 1: an empty clip of the configured length in the selected slot of the selected track, playing | yes, OSC `/track/<n>/clip/<m>/hasContent|isPlaying`, `/overdub/launcher` (with `--fresh-project`) |
| Record clip (`record_note_clip`) | session pad on an empty slot of a rec-armed track (default setting "Record"): JAM session (`framework/view/session_view.rs:handle_recording`), Mk2 PATTERN + pad (`mk/view/clip_view.rs:execute_function`), LP session | Maschine `ClipView`, Electra One `SessionMode`, and all `AbstractSessionView` users | JAM: arm track 1 (REC + A), press its empty slot in session mode: the slot records | yes, OSC `/track/<n>/clip/<m>/isRecording|isRecordingQueued` |
| Clip loop start/length (absolute) | LCXL Drum Sequencer, Rec Arm active: Track Control row sets the loop length (`launchcontrol/mode/buttons.rs:XlLoopLengthMode::on_button`); JAM/LP sequencers: hold a page pad and press another (`framework/view/sequencer/abstract_note_sequencer_view.rs:188`, `abstract_drum_view.rs:433`); LP Pro clip length view | Push, APC, Kontrol, Maschine, Launchpad | LCXL template 4: press Rec Arm (toggle on), press a Track Control button (per the manual; the mode reads `row 0`, verify which row): the clip loop becomes that many steps | yes, probe `/clip/loopStart`, `/clip/loopLength` |
| Clip play start/end, loop start/length relative (`clip start|end|loopStart|loopLength/inc`) | none (`cursor_clip.rs:change_play_start/end`, `change_loop_start/length` have no caller; the Maschine loop modes change the arranger loop) | Push `ClipMode` (play start/end), Kontrol Mk2 protocol, Maschine Mk3 | – | probe `/clip/start`, `/clip/end` |
| Quantize, transpose, clear all, double content | Mk2 Shift+pads 5/6/9/13-16 (`mk/view/shift_view.rs`); JAM shift view (`jam/view/shift_view.rs`), GRID (quantize); JAM SHIFT+DUPLICATE, LP Shift+Duplicate (`duplicate_content`) | Maschine, Push, Launchpad, Kontrol, Launchkey, … | Mk2: hold STEP LEFT, press pad 9: all notes of the selected clip are deleted | yes, OSC `/clip/step/…` (conformance module) |
| Step velocity spread | LCXL sequencers: Rec Arm held + Send B knobs (`launchcontrol/mode/main.rs:189`, `note_parameter.rs`) | Push, Fire, Maschine, LCXL, Launchpad, SL MkIII, OXI, Yaeltex, APC (`NoteParameter`) | LCXL template 4 with a step on: hold Rec Arm, turn Send B knob 1 | yes, `/clip/step/…` field 5 |
| Step pitch | LCXL Note Sequencer Send A knobs (`note_parameter.rs:move_note`) | as above | LCXL template 5: turn Send A knob 1: the note of step 1 moves in the scale | yes, the note of `/clip/step/<ch>/<step>/<note>` |
| Step duration | Mk2 Events mode, knobs 1-2 (`mk/mode/edit_note.rs:change_value`) | as above | Mk2: STEP LEFT+Select (Events), select a step pad, turn knob 1 | yes, field 3 |
| Step gain, pan, timbre, pressure, transpose, release velocity, chance, occurrence, recurrence, repeat curves | none on Bitwig: the Mk2 edit note mode and the LP note edit view still allow only the Reaper attributes (`mk/mode/edit_note.rs:NoteAttribute::is_supported`: duration, velocity; `launchpad/view/note_edit_view.rs:NoteAttribute::is_supported`: velocity, mute; known, BITWIG_DIVERGENCES.md "Note attributes on Bitwig") | Push, Fire, Maschine `EditNoteMode`, Launchpad `NoteEditView`, SL MkIII, OXI, Yaeltex, APC | after the fix: Mk2 Events, LP note edit (long press a step) | yes, fields 6-11, 13-23 |

## 8. Not used by any ported controller (dead on Bitwig until another controller is ported)

| Function | Would become testable by porting |
| --- | --- |
| Cue markers (add, launch, select, remove, previous/next) | Push, MCU, APC40, Electra One |
| Crossfader (master/transport `crossfade`), track crossfade mode A/B | Push, APC40, BeatStep, ACVS, SL, Yaeltex Turn, EC4 |
| Time signature | no Java controller (only the OSC module) |
| Pre-roll measures, pre-roll metronome | Push (`MetronomeMode`) |
| Action invoke | MCU, EC4, Electra One |
| Track move, device move (swap with previous/next) | Push, MCU (tracks) |
| Groove accent amount, rate, phase | Push (`GrooveMode`), OXI One |
| Metronome ticks | Fire, Kontrol Mk1, MCU, OXI One; or registering the Maschine Studio (code ported, `maschine/mk/mod.rs:SUPPORTED_MODELS`) |
| Device remove, duplicate; commands of the first instrument, drum and EQ device without index; sibling commands and sibling paging | Fire (remove), Push, MCU, SL MkIII, Kontrol |
| Clip play start/end and relative loop changes | Push (`ClipMode`), Kontrol Mk2 |
| Step gain, pan, timbre, pressure, transpose, release velocity, occurrence, recurrence, repeat curves | no porting needed: lift the Reaper limit in `mk/mode/edit_note.rs` and `launchpad/view/note_edit_view.rs` (as done for the LCXL) |
| Remote controls section visibility (LP scene 3: a no-op in `device.rs`) | already called by the Launchpad; the device function is not ported |

## 9. Suggested test order

1. **Maschine JAM, by hand and then as scenarios** (most functions, all observable): SELECT+NOTE
   REPEAT (fill, probe `/fill`), SELECT+SWING / SHIFT+SWING / SWING+encoder (groove, probe),
   IN 1 + encoder (`/click/volume`), HEADPHONE + encoder and encoder press (cue, probe),
   SELECT+SHIFT+PLAY (`/punchOut`), SHIFT+SONG (`/layout`), SHIFT+GRID / SELECT+GRID
   (`/track/1/recordQuantization`), MST + encoder (relative inc, `/master/volume`), GRP + encoder
   and press (arranger position and loop, probe `/beats/…`). These change only transport/project
   settings and are easy to reset in `before` with OSC.
2. **JAM, project changes** (with `--fresh-project`): SELECT+MACRO/LEVEL/AUX (add track),
   CONTROL and SELECT+CONTROL (project/track remote controls: give the fixture remote control
   pages first), SHIFT+REC (new clip), a session pad on a rec-armed empty slot (record clip),
   BROWSE (browser), SHIFT+CONTROL (window, needs a plug-in).
3. **Maschine Mk2**: Pin and Active in Volume and Parameters mode (pinned track/device, track
   activation, bypass), PLUG-IN and SELECT+PLUG-IN (window, expand), ARRANGER / Shift+ARRANGER,
   CONTROL (add track), knobs in Volume mode (relative inc), ERASE + knob touch (reset: check first
   that the knobs send touch), SWING ×2/×3 (arranger loop), Shift+pad 9 (clear all), Events
   (duration).
4. **Launch Control XL**: Rec Arm + Track Focus 1 (new clip), Rec Arm + Track Focus 8 (window),
   Device + Send Select (track/project parameters), Track Focus held + fader 7 (metronome
   volume), Rec Arm + Send B in template 4 (velocity spread), Send A in template 5 (pitch), Rec
   Arm toggled + Track Control (loop length, probe `/clip/loopLength`).
5. **Launchpad (if available)**: device view scenes 1/2/4/5, user view, shuffle view, Shift view
   add track pads; engine and panes only on the Pro Mk3.
6. Not testable with the user's hardware: markers, crossfader, time signature, pre-roll, action,
   track/device moves, groove accents, metronome ticks, device remove/duplicate, siblings, clip
   play start/end, indications (look at Bitwig's GUI only).
