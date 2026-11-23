# DrivenByMoss4Reaper → Rust: Port Status

As of 2026-11-02 · branch `main`

## Summary

Seven controller families are ported: Novation Launchpad (5 models), Novation Launch Control XL,
NI Maschine Mk2, Mk3, Mikro Mk3, Plus and Jam, Mackie MCU and HUI (with extenders), Generic Flexi, and the Akai APC mini,
APC40, MPC/Force (ACVS) and Fire, and the Electra One (Mk1/Mk2, Mini). The crate has about 90,000 lines of Rust in 315 files. 372 tests
pass, 2 are ignored, and `cargo clippy --all-targets` reports 0 warnings. The release build
produces `libdrivenbymoss.so` (about 6.5 MB), which exports the 25 `dbm_*` functions of the
handle-based C ABI.

**First successful test in Reaper (2026-10-29):** the Reaper extension in `backends/reaper`,
switched from the JVM to the C ABI through the C++ wrapper (`bindings/cpp`), was built on Linux,
loaded in Reaper and tested with a Novation Launch Control XL, including template switching. The
Maschine Mk2 and JAM work in Reaper on Linux through `dbm-nihost` (see below). The Launchpad has
not been tested with hardware yet.

The CMake paths for Windows and macOS are written but untested. The Visual Studio, Xcode and
Eclipse project files in `backends/reaper` still reference the removed JVM files; CMake is the
supported build.

Building the extension needs CMake, a C++ compiler and Rust:

```
cd backends/reaper
cmake -S CMake -B build -D CMAKE_BUILD_TYPE=Release
cmake --build build
```

CMake runs cargo itself and links the core statically into `reaper_drivenbymoss.so` (`.dll` on
Windows, `.dylib` on macOS).

The protocol between the core and a host (commands, model updates, call sequence) is written down
in [BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md), with a list of mismatches between the Rust core
and the Reaper backend. [BITWIG_BACKEND_PLAN.md](BITWIG_BACKEND_PLAN.md) drafts a Bitwig Studio 6.1
backend (a Java extension over FFM) with a dev mode that reloads the rebuilt native library.

`ci/linux.sh` builds and tests everything that builds on Linux: the crate (with the replays of the
recorded Bitwig sessions), the C++ wrapper, `dbm-nihost`, the Reaper and the Bitwig extension.
`.github/workflows/linux.yml` runs it on every push, after `cargo clippy --all-targets -D warnings`
and `shellcheck` of the own scripts. Next: a Windows build (the Reaper extension; `dbm-nihost` with
hidapi would give Windows users the Shift button of the Mk2, which NI's driver does not send in
MIDI mode).

| Controller | Models registered | Rust path | Integration tests |
| --- | --- | --- | --- |
| Novation Launchpad | MkII, Pro, Pro Mk3, X, Mini MkIII | `src/controller/launchpad/` | `launchpad_smoke.rs` (2), `launchpad_setup.rs` (10), `launchpad_views.rs` (11) |
| Novation Launch Control XL | XL | `src/controller/launchcontrol/` | `launchcontrol_xl.rs` (12) |
| Novation Launchkey | Mk3 (25-88), Mini Mk3 | `src/controller/launchkey/` | `launchkey.rs` (13) |
| NI Maschine | Mk2, Mk3, Mikro Mk3, Plus, Jam (Studio prepared, not registered: its monitor section is missing) | `src/controller/maschine/` | `maschine_mk2_setup.rs` (15), `maschine_mk3_setup.rs` (10), `maschine_jam_setup.rs` (12) |
| Mackie MCU | MCU with 0-3 extenders (`MCU1..4ControllerInstance`) | `src/controller/mackie/mcu/` | `mackie_mcu.rs` (9) |
| Mackie HUI | HUI with 0-2 extenders (`HUI1..3ControllerInstance`) | `src/controller/mackie/hui/` | `mackie_hui.rs` (5) |
| Generic Flexi | Flexi | `src/controller/generic/` | `generic_flexi_setup.rs` (14), `generic_flexi_touch.rs` (2) |
| Akai APC mini | Mk1, Mk2 | `src/controller/akai/apcmini/` | `akai_apcmini.rs` (9) |
| Akai APC40 | Mk1, Mk2 | `src/controller/akai/apc/` | `akai_apc40.rs` (6) |
| Akai MPC/Force (ACVS) | MPC Live I/II, One, X, Force | `src/controller/akai/acvs/` | `akai_acvs.rs` (4) |
| Akai Fire | Fire (with the OLED display) | `src/controller/akai/fire/` | `akai_fire.rs` (4) |
| Arturia Beatstep | Beatstep (also the Beatstep Pro, same MIDI) | `src/controller/arturia/beatstep/` | `arturia_beatstep.rs` (9) |
| Electra One | Mk1, Mk2 (one definition), Mini | `src/controller/electra/` | `electra_one.rs` (13); against the emulator: `support/controller_simulator/tests/test_electra_emulator.py` (2) |
| Open Sound Control | OSC (`OSCControllerInstance`) | `src/controller/osc/` | `osc_controller.rs` (1, over localhost UDP), Python `test_osc.py` |
| Ableton Push | Push 1, Push 2, Push 3 | `src/controller/push/` | `push_setup.rs`, `push_modes_track.rs`, `push_modes_misc.rs`, `push_views.rs`, `push_sim_values.rs`, `ffi.rs` (USB callback), Python `test_usb.py`, `test_simulator_data.py`, Zynthian `test_push.py` |

There are also 220 unit tests in the sources.

**Mackie MCU and HUI (2026-11-02):** the first controllers with several surfaces per instance
(the main device and its extenders, each with its own MIDI port pair). The context keeps the
current surface and swaps the others in (`framework/multi_surface.rs`, design in
[MULTI_SURFACE_PLAN.md](MULTI_SURFACE_PLAN.md)); the app resolves and opens one port pair per
surface, the Bitwig extension opens all ports of a definition. Not tested with hardware yet. Not
ported: the "Pin FX tracks to last device" option (needs an effect track bank, which Reaper has
not; the Java version does not activate it in Reaper either).

**Generic Flexi** (2026-11-02): all 1122 commands of the 21 Java handlers, the 300 command slots
with CC (7 and 14 bit), note, program change, pitchbend and MMC triggers, the 10 function layers,
the six knob modes, sending the values back to the device, the keyboard note input with its
filters and the key translation of mapped notes, the program banks (`.programs` file next to the
mapping) and the 12 modes. The mapping files use the Java format (`SLOT<n>_TYPE`, ...), so Java
mappings load unchanged; all Java settings are activated with the same keys. The settings of the
selected slot ("Selected Slot - ...") and the MIDI learn values are settings, so a host with a
settings UI (Bitwig) can edit the mapping. Not ported: the file dialogs and the signal buttons
("Set", "Save", "Load"; `editor::learn_set`, `mapping::save_file` and `mapping::load_file` are
their functions) and the FX track commands (no effect track bank, as in the Java Reaper version).
See the module documentation of `src/controller/generic/` for the deviations.

**OSC (2026-11-10):** the Open Sound Control controller with all 12 Java modules (transport,
global, layout/panels/arranger/mixer, markers, project, tracks, scenes, devices with the
equalizer, layers and drum pads, browser, virtual keyboard and note repeat, actions, cursor clip)
and its settings (receive port, send host and port, value resolution, bank page size, the 20
actions, logging). The new `src/framework/osc/` has the OSC 1.0 encoding (messages and bundles,
argument types i f s S b h d t c r m T F N I), Java's number formatting, the writer which sends
only changed values (framed by `/update`) and the UDP server and client on `std::net`. The
controller polls its non-blocking socket at every tick of the core and sends from there, so it
needs no thread and no new C function; its commands run under the lock of the instance like MIDI.
It has one MIDI input (the virtual keyboard plays into its note input) and no MIDI output: the app
opens no output for a controller without outputs. Tested over localhost with the Rust tests and
the Python reference host, not with Bitwig or a real client (TouchOSC, Open Stage Control) yet;
`backends/bitwig/tools/osc_monitor.py` (stdlib) prints what it sends and sends single commands.
The addresses are a typed table (`src/controller/osc/address.rs`, `macro_rules!`): the incoming
commands are an enum per module, parsed segment by segment with the errors of the Java parser
(guards for the parts Java only reads in a certain state), and the outgoing values are typed keys
whose addresses are formatted once; a tick without changes costs about 30 µs (release, 3 tracks,
1 device; 530 µs with the former string matching and `format!` per value). Golden files of the
former implementation (`src/controller/osc/golden/`) keep the behaviour byte-identical.
Not ported (no command in the core): the crossfade mode of tracks, renaming and entering layers,
`/browser/insertClap`; and the Java `ConformanceModule`. The Bitwig extension definition is not
generated yet (`backends/bitwig/tools/gen_definitions.py`). Divergences: BITWIG_DIVERGENCES.md
§9, bugs kept: UPSTREAM_CANDIDATES.md Q.

**Ableton Push 1, 2 and 3 (2026-11-19):** all modes, views and commands of the Java version,
for the three models (Push 1: the 4x68 character sysex display; Push 2/3: the 960x160 display),
the Push 3 specifics (encoder, extra buttons, MPE with in-tune settings, the audio interface
settings) and all settings of `PushConfiguration`. The Push 2/3 display is drawn by the extended
`framework::graphics` (the large-display components, icons, anti-aliasing, a vector font); every
frame (16-byte header + 327,680 BGR565 bytes, XOR-masked) goes to the host through the new
`Backend::usb_bulk_out` / C ABI `dbm_set_usb_callback` (changed frames at most 30 per second,
a keep-alive every second). The hosts write it with Bitwig's USB API (the generated definitions
claim the device), with usbdevfs from Python (Zynthian, the reference host) or to the UNIX socket
of the Push simulator (`DBM_PUSH_DISPLAY_SOCKET`, HOST_BRIDGE_GUIDE §5a). Not tested with
hardware or in Bitwig yet. Not ported: the device favorites of Add Track, the track crossfade,
slot chains, entering layers, the host's MPE note input; see BITWIG_DIVERGENCES §9, bugs:
UPSTREAM_CANDIDATES R.

**Akai (2026-11-02):** APC mini Mk1/Mk2, APC40 Mk1/Mk2, MPC/Force over ACVS and the Fire.
The Fire's 128x64 OLED is drawn with the new dependency-free graphics module
(`src/framework/graphics/`: RGB bitmap, graphics context, embedded 5x8 bitmap font, the
components of the small displays and a display with notifications) and sent as 1 bit stripes.
Not tested with hardware yet. Not ported:
- APC40: the track crossfade (Reaper has no crossfade parameter per track) and the simulator
  colors.
- Fire: the XoX drum view (needs `AbstractDrumXoXView`), pinning the cursor track and the knob
  sensitivity in the browser.
See the module documentation in `src/controller/akai/` for the details and the Java deviations.

**Arturia Beatstep (2026-11-10):** the Beatstep and the Beatstep Pro (one definition, as in
Java), with all 8 views of the Java package: track, device, play, drum and note sequencer,
session, browser and the shift view (mode selection and transport). The pad LEDs are sysex
messages; the pads play notes through the note input of MIDI channel 3, the hardware sequencer
has its own note input (MIDI channel 1, the Shift note blocked). The 17 relative knobs (offset
binary) go to the active view, limited to ±10 steps per message as in Java. Registered in the
Bitwig extension ("Arturia Beatstep (Rust)"). Not tested with hardware or on Bitwig yet; the
conformance scenarios (`tests/conformance/beatstep/`, 21 scenarios) need a Bitwig profile
`beatstep`. Not ported: the Beatstep Pro step buttons (`StepCommand` is ported but, as in Java,
not registered), the 1 ms pause after each pad sysex (BITWIG_DIVERGENCES.md §9), the crossfade
mode knob of a track (Reaper has no crossfade parameter per track). See the module documentation
in `src/controller/arturia/beatstep/` for the details.

**Novation Launchkey (2026-11-10):** the Launchkey Mk3 (25, 37, 49, 61 and 88 keys, one
definition as in Java) and the Launchkey Mini Mk3, as one setup with the model as a parameter
(the Java `maxi` and `mini` packages are mostly copies). Ported: the DAW mode handshake (the Mk3
waits for the device to confirm, then synchronizes the knob, fader and pad modes), the device
inquiry (the Mk3 detects the 88 key model, whose display has another device ID, and the models
without faders), the 36 row display of the Mk3 (base, knob and fader screens, ISO-8859-2), the
three pad mappings (session, drum, device select), the session view with the pad modes of the
lower row, the pad mode select view, the drum view with its step sequencer and configuration,
the device select and browser views of the Mk3, the knob and fader modes, the custom modes, the
buttons below the faders (select / rec arm, with Undo delete) and the Mini's user knobs (project
or track parameters on the keyboard port) and program changes. The keyboard port is the second
MIDI input of the definition: a new, opt-in generic part, `ControllerFactory::num_extra_midi_inputs`
(further inputs after the port pairs, configured as `MIDI_INPUT<n>`, detected with the further
input names of a discovery pair, numbered after the pairs on Bitwig; their MIDI goes to
`ControllerSetup::handle_extra_input_midi`), as the ACVS does for its pads port. Not
tested with hardware yet. Not ported: the simulator lights and layout. The Launchkey Mk4 has no Java implementation.
See the module documentation of `src/controller/launchkey/` and BITWIG_DIVERGENCES.md §9.

**Electra One (2026-11-22):** the Electra One Mk1/Mk2 and the Electra One Mini, as one setup with
the variant as a parameter (the Java `base` package with `normal` and `mini`). Ported: the
conversation on the CTRL port (device information with the firmware and model checks, the event
subscription, the preset list with the `DrivenByMoss` / `DrivenByMoss Mini` preset, the preset
switch, online/offline with the Dummy mode, SysEx chunks joined until F7, the device's log lines),
the page cache (values as CCs on channel 16, names/colors/visibility as JSON, value texts, group
labels with the preset's Lua `sgt`, every flush framed by a repaint pause), the 7 modes of the
Mk1/Mk2 (mixer, sends, device, equalizer, transport, session with its function and navigation
layers, project/track parameters) and the 3 of the Mini, the knob touches (the parameter is
touched) and the 38 (Mini: 16) knob touch combinations with the configurable functions, the pads
and the Mini's knob pushes and buttons (Shift on button 4). New generic parts: a JSON reader
(`json::JsonValue`), further MIDI outputs of an instance (`ControllerFactory::
num_extra_midi_outputs`, `create_with_extra_ports`, next to the further inputs; the CTRL port is
the further input and output), the play position, tempo, cue volume and groove parameters
(`parameterprovider::transport`), an emulated Shift (`Surface::shift_pressed_override`) and the
note input filter `?X????` (all channel messages of a channel). `MasterVolumeMode` is not needed:
the Electra only shares the error text of its focused parameter check. Tested against the
**Electra One emulator** (the real mk1 firmware on Linux, `~/bastel/github/electra-one`, mk1 and
mk2 replies): `dbm_host_alsa.py --model electra` drives it over ALSA (both port pairs), and
`test_electra_emulator.py` runs it headless over its virtual cables (handshake, screen pixels, knob
turn and touch, pad touch, all 7 pages). The Mini is tested with unit tests only (the emulator
runs the mk1 firmware). Not tested with hardware. Java bugs kept: UPSTREAM_CANDIDATES.md S;
deviations: BITWIG_DIVERGENCES.md §9.

**Surface link (2026-11-22, not part of Java):** a generic link between two controller instances
in one process ([SURFACE_LINK.md](SURFACE_LINK.md)).
- **The framework** (`framework::surface_link`):
  - a process-wide registry, two locked queues per surface (one per direction, each drained on
    its own tick), explicit states (missing/attached/detached/incompatible) and a protocol
    version;
  - typed messages (the 6x6 page model, its changes, the input, the capabilities);
  - the reusable `LinkProvider`, and a generic surface view and mode (`SurfaceLinkView`,
    `SurfaceLinkMode`) driven by a `SurfaceMapping`.
- **The Electra One** (Mk1/Mk2) provides its 7 pages and executes the input as its own knobs,
  pads and touches. With the emulator's capability (electra-one `docs/surface-link.md` §6b) it
  also sets the active row and the selection highlight on the device (`surface_ext.rs`);
  otherwise it only switches the control set.
- **The Maschine JAM** is the first surface: Select + SONG, the 6x6 slots on the grid, the rows
  and pages on the outer columns, the strips for the active row, left/right for the value
  (handle) of the selected slot.
- **The generic provider** (Electra One Mk2 emulator only, electra-one §6c): while another preset
  than DrivenByMoss's is active, the Electra provides that preset's current page (`02 52`, one
  slot per cell = knob, `7E 62`/`7E 63`) and the surfaces turn its knobs (`14 53`-`14 56`); the
  DAW side is offline meanwhile. Tested end to end with the emulator (Acid Machine, Euclidean and
  the emulator's surface-view fixture) and the JAM simulator.
- **Colors:** the slots take the nearest entry of the surface's palette (CIEDE2000), not the
  detour over Bitwig's track colors. The row selectors alternate per control set only on a
  surface whose encoders cover a whole set (12+), the JAM's inactive rows have one color.
- **Tests:** unit tests, `tests/surface_link.rs` (both controllers as two `App`s in one test),
  and end to end with the emulator, the JAM simulator and `dbm_host_alsa.py --model electra
  --model jam`. `Views::SurfaceLink` and `Modes::SurfaceLink` are appended to the generated ID
  enums.

**Colors without Bitwig (2026-11-22, not part of Java):** an RGB color which does not come from
Bitwig goes to the nearest entry of the controller's own palette (CIEDE2000), not over the
closest of Bitwig's ~25 track colors and Java's table for them (orange F49500 was lime green 22 on
the NI palette, now an NI orange). "Bitwig" is the host's `app_version` starting with "Bitwig"
(the Bitwig extension, the conformance runs and their replays): there everything stays as in
Java, a test checks it for every controller, the conformance suite stays at 195 same. The other
hosts (reference host, zynthian, Reaper) and the surface link take the direct path. Palette
tables: NI Maschines, Push 1/2/3, APC40 mkII, Launchpads, Launchkey Mk3; the others keep the DAW
color path (FRAMEWORK_MAP.md, "framework/controller/color"). Next step: the Java tables from the
DAW colors to the palette indices (e.g. `DAW_COLOR_LIGHT_ORANGE` → `COLOR_YELLOW` 22), which the
Bitwig path still uses.

**Maschine LED brightness (2026-11-23, not part of Java):** on the real JAM the LEDs look dim
compared with the power-on animation: the RGB LEDs only take an index into NI's palette
(`hue * 4 + level`, no brightness byte), and many states use the dim or medium level (Java's
`*_LO` colors, dark grey, and the nearest palette entry of the direct color path). The global
setting "LED Brightness" (Hardware Setup, `HARDWARE_SETUP_LED_BRIGHTNESS` in the `.config` file)
of all Maschines has Normal (default, as Java) and Bright: Bright raises only the dimmest palette level (`BRIGHT_LEVELS = [1, 1, 2, 3]`: dim to medium;
medium, full, pale, black and the whites 68..79 stay, so inactive (medium `*_LO`), active (full)
and highlight (pale) stay distinct, as in Normal) and the single-colour values (0 and 127 stay; the core sends only those) where the LEDs are sent
(`maschine::core::brightness`, the trigger senders, the pad grid's color mapper, the JAM's strip
setup), for all modes, views and the surface link. A change re-sends all LEDs. Bitwig shows it in
the controller preferences; on zynthian it is set in the `.config` file of the model in
`$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/` (read when the driver starts). The levels come from an LED
test on the real JAM (`dbm-nihost-led-test --pattern rainbow --level N`): full and pale are both
bright enough for studio use (pale the brightest), the dim look comes from the dim and medium
levels; an earlier Bright raised every level by one step (active full became the whitish pale
and merged with the highlight), now active stays full and highlight pale; 68..79 are all the same white
(cabl's grey steps do not exist). The JAM drives its LEDs in time slices (about 25% duty), so
even pale stays dimmer than its startup animation; a device-wide brightness setting is not known.
The JAM's feature report 0xF1 is its pad palette, stored in flash: a possible custom palette, not
written by DrivenByMoss (flash wear).
dbm-nihost's curve for the JAM's mono LEDs now maps MIDI 1..127 to hardware 2..85 (gamma 1.5;
1 is off on the device, 85 is full), see `support/linux/ni_host/README.md`. Tests: the mapping
(unit tests), `led_brightness_raises_the_palette_levels_and_re_sends_all_leds` (JAM, Mk2).

Commits:

- `e00a36ec`: core framework, Reaper DAW layer and Launchpad
- `a3bb3b89`: Launch Control XL and Maschine Mk2/Jam, with the parameter providers, the
  parameter, device, track and layer modes, the text display and more generic commands

**Maschine on Linux:** NI's driver doesn't exist on Linux. `support/linux/ni_host` emulates it
for the Mk2 and the JAM (hidraw on one side, NI's template MIDI on the other), as the library
`libdbm_nihost.so` and as the daemon `dbm-nihost` with ALSA ports with NI's names. The Linux
guard of the Maschine factories is removed.
- **In-process HID is the default** (`src/nihost/`): in a host which allows it
  (`DBM_FLAG_DEVICE_TRANSPORT`: the Bitwig extension, the Python reference host
  `dbm_host_alsa.py`) the core loads `libdbm_nihost.so` with `dlopen` and drives the device
  itself: no dbm-nihost, no snd-virmidi, no MIDI ports to assign. The core reads the device on
  its tick and on `dbm_poll_devices` (Bitwig: every 3 ms), writes the LEDs and displays after
  each flush and applies the note inputs itself (the pads play through `process_midi_arg`).
  `DBM_MASCHINE_TRANSPORT=hid|midi|auto` overrides the choice; auto falls back to the MIDI ports
  (logged) without the library, while dbm-nihost drives the device, or while the hidraw node is
  locked by another process or not accessible. The zynthian drivers keep their own in-process
  path (ctypes, `dbm_zynthian/nihost.py`) and don't pass the flag; the conformance runs set
  `DBM_MASCHINE_TRANSPORT=midi`. Tested with fakes, the library's test devices and the selection
  in the Python host (2026-11-23); not yet with a connected Mk2 or JAM, nor in Bitwig.
- With dbm-nihost (the MIDI ports), as before (Reaper, and Bitwig without the in-process transport):
  - Tested on both devices with the Rust port in Reaper, and with the Java DrivenByMoss in Bitwig.
    The Java version needed its Linux check removed (local branch `linux-maschine` in the
    DrivenByMoss repository).
  - Reaper sees the ports through PipeWire's MIDI bridge (`Midi-Bridge:Maschine MK2: In (capture)`);
    the discovery pairs include these names.
  - Bitwig only opens raw MIDI devices; `dbm-nihost --virmidi` connects the ports to the devices of
    `snd-virmidi`.
  - It installs with CMake, including a systemd user service. See
    `support/linux/ni_host/README.md`.

**Maschine Mk3, Mikro Mk3 and Plus:** registered (`SUPPORTED_MODELS`), with the definitions and
discovery names of Java (Windows and Linux `Maschine MK3 Ctrl MIDI` / `Maschine Mikro MK3`, macOS
`Maschine MK3 Virtual Input`/`Output`, `Maschine Plus Virtual`) and the Bitwig definitions. The
shared setup has the model differences of Java: NI's Shift SysEx (Mk3, Plus) or Shift on Stop
(Mikro), the cursor keys and page buttons of the 4-D encoder, two foot switches, the MCU display
(not on the Mikro), 16 tracks per bank and no knobs, group buttons or display buttons on the
Mikro. Tested with `tests/maschine_mk3_setup.rs` and against the Maschine simulator (in-process
and over ALSA), not with hardware: there is no dbm-nihost for them, so on Linux they need NI's
driver's port names from elsewhere (the simulator creates them). The Mk3's touch strip touch is
disabled in the DrivenByMoss NI template, so with NI's driver the pitch modes don't return to the
centre (CONTROLLER_SIMULATORS.md §10). The Studio is not registered: its monitor section (the
monitor encoder with its modes and VU, MST/GRP/SND/CUE/IN1-4, the cue and metronome volumes) is
not ported.

**Push simulator:** `support/push_simulator` is a PySide6 + QML desktop simulator of Push 2 and
Push 3 (Push 1 with its text display), for testing the Push port without the hardware. It creates
ALSA sequencer ports with the device's names (`Ableton Push 2 Live Port` / `User Port`; ctypes on
libasound, no other packages) and implements the device side of the protocol: pads with velocity
and aftertouch (Push 3 MPE), buttons, relative encoders with touch, the touch strip with its LED
modes, LED colours from the palette with the blink/pulse channels, the palette and settings SysEx,
and the identity reply. The display frames arrive on a UNIX socket
(`$XDG_RUNTIME_DIR/dbm-push-display.sock`) as the exact Push 2 USB bulk stream (header
`FF CC AA 88`, 160 × 2048 bytes BGR565, XOR `E7 F3 E7 FF`). `tools/dbm_host_alsa.py` runs a core
controller in the reference host `dbm_host` against it in real time, and `--connect` reaches
Bitwig through `snd-virmidi`. Unit, ALSA loopback and offscreen smoke tests are in its `tests/`.
See `support/push_simulator/README.md`.
A simulator-only side channel (`dbm_set_simulator_callback`, `DBMV` messages on the display socket,
HOST_BRIDGE_GUIDE §5a) sends the Push 2/3 encoder state from the display (values, names, touch),
which the simulator draws as LED rings (Python hosts and Bitwig with `DBM_PUSH_DISPLAY_SOCKET`).

**Maschine simulators:** `support/controller_simulator` has simulators of the Maschine Mk2 and
JAM in the same style (and of the Mk3 and Mikro Mk3, which emulate NI's driver directly with the
MIDI of the DrivenByMoss NI templates). They speak what DrivenByMoss sees through dbm-nihost: the client and port
names (with the suffix ` (sim)` while dbm-nihost owns them), the template MIDI, the LED feedback
with NI's palette, the Mk2's MCU text displays and button labels, the JAM's touch strips and level
meters, and the hello/query/gone handshake, including the Mk2's Shift. They share `sim_common`
(ALSA, MIDI, Qt base, the QML module `SimCommon`) with the Push simulator. `run.sh --model
mk2|jam|push*`; `dbm_host_alsa.py --model mk2|jam` runs the core against them. How the simulators
are built and what we learned: [CONTROLLER_SIMULATORS.md](CONTROLLER_SIMULATORS.md).

**APC40 mkII and Launchpad Pro Mk3 simulators:** `midi_simulator` in `support/controller_simulator`
(`run.sh --model apc40mk2|lppromk3`, `dbm_host_alsa.py --model apc40mk2|lppromk3`): the APC40 mkII in
Ableton Live mode with RGB pads and scene buttons (animation channels), single LEDs, absolute knobs
with LED rings from the ring CCs, faders and crossfader; the Launchpad Pro Mk3 with its three ports,
programmer mode, pads with aftertouch, all buttons, flashing/pulsing LEDs and the SysEx LED update
(RGB, logo). Their tests include a run of the real core in-process, which checks that every message
of the core is understood and every control is handled.

**Launch Control XL simulator:** `midi_simulator --model lcxl` (also `run.sh`, `dbm_host_alsa.py`):
the 16 templates with the core's template-change handshake (the simulator answers like the device)
and a template selector, 24 knobs with their two-colour LEDs, 8 faders, all buttons with LEDs,
flashing and the SysEx LED update; tested like the others, including the in-process core run.

**Scope and size:**

- The Java project has about 240,000 lines in 1,544 files.
- The crate is self-contained in `rust/`: a single crate `drivenbymoss` (cdylib + rlib) with no
  external dependencies. The Java tree is unchanged.
- It can be split into its own repository with `git subtree split --prefix=rust`.

**Not ported:**

- The Swing/JavaFX UI and the simulator windows.
- USB, HID and the gamepad. Graphics displays: only the Akai Fire OLED (MIDI system
  exclusive); the USB displays (Push 2/3) are not ported.
- All controllers not listed above. See [PORTING_GUIDE.md](PORTING_GUIDE.md) for the
  remaining ones.

## Architecture

The Rust library replaces the embedded JVM.

- **Calls in:** the `dbm_*` functions in `src/ffi.rs` mirror the static methods of the Java
  `Controller` class, but take an instance handle:
  - `dbm_create` / `dbm_destroy`
  - `dbm_update_model`, `dbm_on_midi_event`, `dbm_tick`
  - `dbm_get/set_formatted_document_settings`, and so on
- **Header:** `include/drivenbymoss.h` is generated by cbindgen in `build.rs`; the
  configuration is in `cbindgen.toml`. See [BINDINGS_PLAN.md](BINDINGS_PLAN.md) for the plan
  for C++, Python and Java bindings.
- **Calls out:** the `DbmCallbacks` table of function pointers replaces the JNI `native` methods
  of `MainApp`: `process_*_arg`, MIDI open/close/send, and the note-input filters and
  translation tables. Each pointer may be NULL, which means it is ignored.
- **Threading:**
  - Each instance serializes its calls with its own mutex, so several instances can run in one
    process.
  - A background thread per instance calls `App::tick()` every 30 ms, like the Java flush
    timer. With `DBM_FLAG_HOST_TICK` there is no thread, and the host calls `dbm_tick()`
    itself.
  - Inside the lock everything is single-threaded (`Rc`/`RefCell`, no `Arc`).
  - Panics are caught at the FFI boundary.
- **Context passing:**
  - In Java, views, modes and commands keep references to the surface and the model. In Rust
    they get `&mut Ctx` (`src/framework/ctx.rs`).
  - `Ctx` holds the host, sender, model, configuration, surface, view and mode managers, color
    manager, scheduler, an optional text display, and a typed `ext` slot for controller state.
- **Events instead of observers:** the model, the configuration and the parser record changes
  (`take_events()`, `take_changed_settings()`, `ParserEvent`). The controller setup handles them
  in `handle_changes`, before each flush.
- **Inheritance becomes composition:** Java template methods became hook traits on composed base
  structs, e.g. `SessionViewBase` + `SessionHooks`.
- **Flush cycle:**
  1. The active view updates its controls and draws the grid.
  2. Only changed light states are sent to the hardware.
  3. The controller-specific flush runs (Launchpad blink sysex, Maschine emulated blinking,
     text displays).
- **Settings:** stored in the same `.config` property files as Java, so existing user
  configurations keep working. Project settings use the same base64 document format.

## Directory layout

All paths are under `rust/`.

| Path | Java origin | Status |
| --- | --- | --- |
| `Cargo.toml`, `include/drivenbymoss.h` | `build.gradle`, JNI interface | Done |
| `src/ffi.rs` | `Controller`, `MainApp` natives | Done |
| `src/app.rs` | `MainApp`, `ControllerInstanceManager`, `AbstractControllerInstance` | Done |
| `src/backend.rs`, `src/host.rs` | `BackendExchange`, `Processor`, `IHost` | Done |
| `src/midi.rs` | `reaper/framework/midi/*`, `DeviceInquiry` | Done |
| `src/config/` | `AbstractConfiguration`, Reaper settings, controller configurations | Done (Launchpad, Launch Control, Maschine, Maschine Jam) |
| `src/daw/`, `src/daw/clip/` | `framework/daw/**` + `reaper/framework/daw/**` | Done; clip steps stored sparsely |
| `src/scale/` | `framework/scale/**`, `KeyManager` | Done |
| `src/parser.rs` | `MessageParser` | Done |
| `src/framework/` core | ids, value changer, color, grid (incl. emulated blinking), surface, feature (View/Mode traits), command traits, ctx, virtual fader, chord identifier, text display (`display.rs`) | Done |
| `src/framework/parameterprovider/` | `framework/parameterprovider/**` | Done |
| `src/framework/mode/` | track, parameter, device, layer, master, sequencer, dummy modes, note editor | Done |
| `src/framework/commands/` | Trigger, continuous, mode, transport and misc commands, `MVHelper` | Done for everything the 3 controllers use |
| `src/framework/view/` | Play, chord, piano, session, scene play, shift, browser, Drum64, tempo/shuffle, transpose views | Done |
| `src/framework/view/sequencer/` | Sequencer, note/poly/raindrops, drum, Drum4/8, clip length views | Done |
| `src/controller/mod.rs` | `IControllerDefinition`, `IControllerSetup`, controller registry | Done |
| `src/controller/launchpad/` | `controller/novation/launchpad/**` | Done |
| `src/controller/launchcontrol/` | `controller/novation/launchcontrol/**` | Done |
| `src/controller/maschine/{core,mk,jam}/` | `controller/ni/maschine/**` | Mk2 and Jam done |
| `src/controller/mackie/{mcu,hui}/` | `controller/mackie/**` | Done (MCU 1-4 devices, HUI 1-3 devices) |
| `src/framework/multi_surface.rs` | `AbstractControllerSetup.surfaces`, connected mode managers | Done |
| `bindings/cpp/` | — | Header-only C++ wrapper (`dbm::App`, `dbm::Backend`) and its test |
| `tests/` | — | Integration tests per controller |
| `backends/reaper/` | [DrivenByMoss4Reaper-Backend](https://github.com/git-moss/DrivenByMoss4Reaper-Backend) (C++ Reaper extension) | Squashed git subtree of upstream `master` (0e292a08), switched from JNI to the C ABI. Update with `git subtree pull --prefix=rust/backends/reaper https://github.com/git-moss/DrivenByMoss4Reaper-Backend master --squash` |
| `support/push_simulator/` | — | Push 1/2/3 simulator (PySide6 + QML, ALSA through ctypes, display socket) |
| `support/controller_simulator/` | — | Maschine Mk2/JAM, APC40 mkII, Launchpad Pro Mk3 and Launch Control XL simulators and the shared simulator code (`sim_common`); guide: `doc/CONTROLLER_SIMULATORS.md` |
| `doc/` | — | This file and the porting guide |

## Next steps and open issues

**A typed protocol between core and hosts** (decided 2026-11-08; done for all hosts but Reaper, see
"Status" below). Before it every address
(`/track/3/volume`, host → core) and every command (`track 3/mute 1`, core → host) was a string
that was formatted on one side and parsed on the other.
- **The problems:**
  - An address or command one side doesn't support only shows at run time, as "not handled".
  - The places that use an address are hard to find.
  - Formatting and parsing strings costs CPU in every host. The Zynthian drivers spend most of
    their tick in Python formatting and comparing ~950 values; see ZYNTHIAN_CPU.md.
- **The plan:**
  - Enums for all paths and commands, defined in one table in the core. The C header and the
    bindings (C++, Python, the Bitwig extension's Java) are generated from that table.
  - A binary message type: an enum ID, indices (track, send, slot, …) and a typed value.
  - The text form stays for logs, the protocol recordings (`.rec`) and the replays, with a
    conversion between both forms.
- **Scope:** `src/parser.rs`, the commands in `src/backend.rs` and the `daw` types, the C ABI
  (`src/ffi.rs`, `include/drivenbymoss.h`), the hosts (Bitwig `backends/bitwig`, the Python
  binding and reference host, the Zynthian adapter), the recorder and the replay test. Reaper is
  out of scope: not a target of the project, its backend stays on the text protocol.
- **Migration:**
  - The hosts move one by one, keeping the text protocol until the last one has moved.
  - The conformance replays must stay valid, recorded or converted.
- The plan: [TYPED_PROTOCOL_PLAN.md](TYPED_PROTOCOL_PLAN.md) (BACKEND_PROTOCOL.md describes the
  current text protocol).
- **Status:** steps 0a, 0b and 1 done (the command and the address table in `src/protocol/`,
  the binary records, the binary C ABI next to the text one, the generated `dbm_protocol.h` and
  C++ `dbm_protocol.hpp`, the replay passes in both forms; the grid ID in two slots with
  channel and row packed into one; plan §6). Step 2 in progress: the generated Python
  `protocol.py`, the binary API of the Python binding, the reference host and the Zynthian
  adapter on the binary path (reports and commands; the dual run against the text passes all
  suites); the reporters on int keys (one dict, the collectors by key bases; the Zynthian idle
  pass 17 % cheaper, level with the text pass); the commands dispatched by `CmdId` (typed
  handlers for the channel and parameter commands, the rest through the text; the coverage test
  and the handled set). Step 3 done: the Bitwig extension on the binary protocol (the generated
  `Protocol.java`, the reporter on `long` keys, the commands by an exhaustive switch over
  `Protocol.Cmd`, `DBM_HOST_PROTOCOL=check|text`; the Bitwig conformance runs the same as before,
  plan §6.5). Step 5 done: the text path retired in the hosts (plan §6.6): exact doubles (no
  6-decimal rounding), no text or check mode (`DBM_HOST_PROTOCOL` is gone), every command of
  the Python hosts by a typed handler, report again by entity, the command buffer of Python as a
  `memoryview` (zero copy); the command features of a binary host (`soloToggle`,
  `transportCommands`, `deviceCommands`, `layoutStep`, `clipLauncherNavigator`) come from its
  handled-commands bitset, `/host/features` stays for the value operations (`valueInc`,
  `valueReset`, `immediately`, `indication`) and for text hosts (Reaper, unchanged);
  `src/parser_old.rs` removed.

1. Test the Launchpad with hardware in Reaper, and test the Launch Control XL and the Maschine
   (Mk2, JAM) in more depth. Compare the behaviour with the Java version.
2. Verify the Windows and macOS CMake builds, and update or remove the Visual Studio, Xcode and
   Eclipse project files.
3. Test the Maschine Mk3, Mikro Mk3 and Plus with hardware (registered and tested with the
   simulator), and port the Studio monitor-encoder section.
4. Port the remaining controllers, following [PORTING_GUIDE.md](PORTING_GUIDE.md).
5. Extract `rust/` into its own repository.

Open issues:

- [ ] Launch Control XL: the lights do not turn off when Reaper quits. Check whether the Java
  version does it, and whether the shutdown path (`dbm_destroy` → controller `exit` → queued
  MIDI → `SendMIDIEventsToOutputs`) still reaches the device at that point.
- [ ] There is no UI. Configuration comes from the `.config` files, and `dbm_display_window` only
  prints the log. Decide whether a UI is needed.
- [ ] Decide how to support USB/HID (Push 2/3 display, Kontrol Mk1) and whether external crates
  are allowed for these. Graphics displays are drawn with `src/framework/graphics/` (bitmap font,
  no anti-aliasing, only the components of the small displays; the Push components, icons and
  anti-aliased drawing are missing).
- [ ] Device browser: presets only, with no filter columns and no device database (DeviceManager).
  This limits device and drum-pad browsing (Maschine).
- [ ] No parameter renaming or mapping (DeviceManager), and no device layers (Reaper has none).
- [ ] The knob sensitivity setting is not supported (affects slow knobs with Shift on the
  Maschine Mk3).
- [ ] Note attributes are limited to what Reaper supports (pitch, duration, mute, velocity). With
  the step grid of the host (Bitwig) the Launch Control XL, Maschine and Launchpad note editors
  support all attributes.
- [ ] Small duplications to clean up: `ArpeggiatorMode` and the scale name lists exist in both
  `config/` and `scale/`.

Host bridge findings (2026-10-31, from writing [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md)), to
investigate:

- [x] Bug: `master solo/toggle` (master commands without a value) arrived through
  `process_no_arg`, whose handler in the Bitwig extension had no `master` case, so it was dropped.
  Fixed 2026-10-31; no ported controller toggles the master's solo (they toggle tracks of the
  bank), so it did not show yet.
- [x] The report of `/track/offset` (host paging) switched on all Bitwig command forms (`/inc`,
  `/reset`, `/immediately`, `solo/toggle`, `transport play|stop|restart`, device commands
  without index, …). Done 2026-10-31: the host reports its command forms with `/host/features`
  (`HostFeature`: valueInc, valueReset, immediately, indication, soloToggle, transportCommands,
  deviceCommands, layoutStep; HOST_BRIDGE_GUIDE.md §2); pinning, cursor track navigation, project
  unsolo/unmute, master activation and the VU clip state follow their own reports; a paging host
  without `/host/features` gets all features.
- [x] Commands the core sends which the Bitwig extension did not handle. Done 2026-10-31
  (BITWIG_DIVERGENCES.md §6 "Commands the extension did not handle"): `clip clear` (`clearSteps`),
  `time signature`, `time loop/start|length` (in beats), `track <p>/movePrev|moveNext` and
  `device <i>/movePrev|moveNext` (`moveTracks`/`moveDevices` as `swapWithPrevious/Next`), markers
  (`marker add`, `marker <i>/launch|select|remove`, reported from a cue marker bank of 100 if
  `numMarkers > 0`), `inifile reaper/prerollmeas` (`preRoll`, reported as
  `/click/prerollMeasures`), `action` (Bitwig action IDs), the crossfader (reported as
  `/crossfade`, the core then sends `transport crossfade` instead of `master user/param/0/value`),
  the track parameters `track <p>/param/…` (remote controls of the cursor track as
  `/track/fx/param/…`, on the pinned track with host pinning). Ignored: `toggleMultiSelect` (as
  Java), `device preset`, `device add/0` (Reaper only). The core does not send `device indicate`
  (the drum views use `drum indicate`). Also fixed: the parser read every marker value as a color
  and logged errors (Reaper too).
- [x] The cursor track's remote controls have no page names and do not follow the page selected
  in Bitwig (the project and device remote controls do); previous/next marker use
  `jumpToPrevious/NextCueMarker` instead of Java's search in the marker bank. Now the extension
  reports `/track/fx/page/…` and handles `track parameterPage`, and previous/next marker search
  the marker bank as `MarkerBankImpl.findAndSelect`. No ported controller uses markers yet, so
  the marker bank is untested in Bitwig.
- [x] Commands whose value the extension ignores: `track <p>/send/<s>/active` and
  `device <i>/bypass` toggle; `track <p>/select 0` is ignored. `send active` is a toggle in
  Reaper too (its value is the send volume), and the core only sends `select 1`; Reaper also
  ignores the value. `device <i>/bypass <value>` now sets the state, as in Reaper.
- [x] Addresses the parser accepts which the extension does not report: `/project/name`,
  `/project/isDirty`, `/track/<p>/depth|vuhold*|overdub|inQuant*`,
  `/device/touchedParam`, `/clip/notes|playposition|all`, `/marker/<i>/endPosition`,
  `/scene/<s>/position|endPosition`, `/master/fx/…`, the Reaper browser and groove
  addresses. The extension now reports `/project/name` and `/project/isDirty` (ProjectImpl).
  The others have no Bitwig counterpart in the Java version: track depth, VU hold, track
  overdub, touched parameter, clip play position and note list, region positions, master FX,
  and the Reaper browser and groove are Reaper-only. The track `inQuant*` comes from the
  application's `/recordQuantization/noteLength`, which the extension reports. The guide
  marks them as Reaper-only.
- [x] Docs lag the code: BITWIG_DIVERGENCES.md §3 (automation) and the "Protocol additions" of
  BITWIG_CORE_PORT_PLAN.md (now split into implemented and planned) are updated (2026-10-31).
- [x] The step gain is reported as `gain () / 2` but written with `setGain (value)`. This is
  the same as in Java (`StepInfoImpl.updateData`, `CursorClipImpl.updateStepGain`, clamped to
  0..1). Checked on Bitwig with the Java Mk2 (2026-10-31): a 0 dB note shows 50 %, 60 % is
  +5 dB, 40 % is -14 dB, and a knob step changes the gain a little. So Bitwig's `gain ()`
  returns twice the scale of `setGain` (0.5 = 0 dB), and the halving is right. No bug. But
  right after a setter the NoteStep object returns the set value in the setter's scale, and the
  extension read the live objects: with fast knob turns the core got half the gain. The
  extension now formats the steps when Bitwig's observer reports them (as
  `StepInfoImpl.updateData`).
