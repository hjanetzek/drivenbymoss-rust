# NI Maschine on Linux: plan

**Goal.** Use the NI **Maschine Mk2** (`17cc:1140`) and **Maschine JAM** (`17cc:1500`) with
DrivenByMoss on Linux, in Reaper and later Zynthian, without NI's driver.

> **Status (2026-10-29).** Steps 1–8 are done. Both devices work with the Rust DrivenByMoss in
> Reaper and with the Java DrivenByMoss in Bitwig; step 9 (in-process) is open. Findings:
> - **Palette:** NI's MIDI palette and the JAM's HID palette are the same (hue × 4 + level), and
>   the JAM shows DrivenByMoss's white (78). On the Mk2, 72–79 show white from dim to bright.
> - **JAM mono LEDs** are nearly full from about 0x30, so they get a quadratic curve.
> - **Pads in the Mk2 template:** colours are on the pad notes with hue/saturation/brightness on
>   channels 1–3, but the active page uses an indexed mode (`color-mode="3"`), so DrivenByMoss's
>   single value on channel 1 is a palette index.
> - **Mk2 display button 7** is an MCU button: note 28.
> - **Shift:** the JAM's Shift is NI's sysex `4D 01/00`. The Mk2 has no Shift in the template.
> - **Reaper** sees the ports through PipeWire's MIDI bridge as
>   `Midi-Bridge:Maschine MK2: In (capture)` etc.; the Rust discovery pairs include these names.
> - **Bitwig** only opens raw MIDI devices. `dbm-nihost --virmidi` connects the ports to the two
>   devices of `snd-virmidi midi_devs=2`. Bitwig must not open the Mk2's own class-compliant
>   MIDI port, which disturbs the HID connection.
> - **Java DrivenByMoss** threw an exception for the Maschine on Linux; the check is removed on a
>   local branch (`linux-maschine`).
>
> See `support/linux/ni_host/README.md`.

**Approach: emulate NI's driver.**
- On Windows and macOS, NI's driver turns the device's USB HID into MIDI ports. It uses the
  numbering of the DrivenByMoss Controller Editor templates: `resources/Maschine Mk2/DrivenByMoss.ncm2`
  and `resources/Maschine Jam/DrivenByMoss.ncmj`.
- The Rust controller code (`src/controller/maschine/`) already speaks exactly that MIDI.
- A C++ component in `rust/support/linux/` does the driver's job: HID reports to template MIDI
  and back.
- The controller code stays as it is. The same component also serves the Java DrivenByMoss
  (Bitwig, Reaper), which allows a direct comparison.

The HID side exists already. KICK (`~/bastel/github/behringer-kick`,
`Kick/modules/kick_controllers`, see its `docs/CONTROLLERS.md` §8–9) drives both devices over
hidraw, with protocol classes checked on the hardware.

## 1. What is reused

| KICK file | Lines | Reuse |
| --- | --- | --- |
| `maschine_jam.{h,cpp}` | 361 | **As is.** JAM protocol: buttons (every change of a report), encoder, touch strips with release, mono, RGB and strip LEDs, dirty-report flush. No JUCE. |
| `maschine_mk2.{h,cpp}` | 410 | **As is.** Mk2 protocol: buttons, main encoder, 8 encoders, pads with velocity from pressure, LEDs, group and pad RGB, both displays as HID output reports `0xE0`/`0xE1`. No libusb. No JUCE. |
| `hidraw_device.{h,cpp}` | 219 | **Port.** Uses `juce::Thread`/`juce::String`; becomes `std::thread` + `poll()`. Same logic: a `/sys/class/hidraw` scan once a second, hotplug, a reader thread, `write()` from any thread. |
| `controller_panel.*`, `panel_state.*`, `kick_*_panel.*` | — | **Not reused.** This is the KICK side (panel messages). Its role is taken by the MIDI translation (§3). |
| `tools/jam_sim.py`, `tools/mk2_probe.py` | — | **Reuse for tests.** The uhid simulator gives a virtual JAM that hidraw finds like the real one. Extend it to the Mk2. |

**Licensing.**
- All of this code is written by the project owner. The protocol tables derive from cabl
  (MIT, (C) 2015 Vincenzo Pacella), and so does the MicroDexed-touch JAM driver it was taken
  from.
- The copied files are relicensed from AGPL-3.0 to **LGPL-3.0**, like DrivenByMoss, keeping
  cabl's MIT notice. The header of each file names its origin.

## 2. Layout

```
rust/support/linux/ni_host/
  CMakeLists.txt            static library + daemon + tests; C++17, no JUCE
  include/ni_host.h         small C API (§4.1), for in-process use by hosts
  src/
    hidraw_device.{h,cpp}   port of KICK's HidrawDevice (std::thread, poll)
    maschine_jam.{h,cpp}    from KICK, unchanged apart from the namespace (dbm::nihost)
    maschine_mk2.{h,cpp}    from KICK, unchanged apart from the namespace
    template_map.h          generated: control -> MIDI message of the NI templates (§3.1)
    jam_host.{h,cpp}        JAM: HID events <-> template MIDI (§3)
    mk2_host.{h,cpp}        Mk2: HID events <-> template MIDI, MCU text -> displays (§3)
    palette.{h,cpp}         NI MIDI-mode colour index <-> JAM hue/level, <-> Mk2 RGB (§3.3)
    font.{h,cpp}            small bitmap font for the Mk2 display text (from cabl, MIT)
    alsa_ports.{h,cpp}      ALSA sequencer ports for the daemon (§4.2)
    daemon.cpp              dbm-nihost
  tools/
    gen_template_map.py     reads the .ncm2/.ncmj XML, writes template_map.h
  tests/                    translation tests with recorded/synthetic reports; uhid end-to-end
  packaging/                udev rule, systemd user service, snd-virmidi setup
```

## 3. The translation

### 3.1 The templates as data

- The `.ncm2`/`.ncmj` files are XML (`<ni-controller-midi-map>`). Each `<button>`, `<knob>`,
  `<wheel>` and `<pad>` element has an `id` (e.g. `Browse`, `BtnGroupA`, `CapTstA`). They also
  carry a `<controller>`, `<note>` or `<polyat>` number with its `<channel>`, `<on>`/`<off>` values
  and `<behavior>`; knobs and wheels have a `mode` (`comp` = relative).
- `tools/gen_template_map.py` turns this into a C++ table: control ID → kind, channel and number
  (plus value range and mode).
- A hand-written table per device then maps each HID button or encoder of KICK's protocol
  classes to a template control ID (`MaschineMk2::Browse` → `"Browse"`).
- The exact numbering the Rust code expects comes from the same files NI's driver uses, so
  nothing is copied by hand.
- Cross-check: the CC and note constants in `src/controller/maschine/*/surface.rs` must match the
  generated table. A test compares them.
- Pages: the templates contain group, matrix and touch-strip pages. The DrivenByMoss templates
  disable switching (`<disableMatrixButtonPages/>`, `<disableTouchstripPages/>`,
  `<handleGroupControls/>`), so only the active page (`current_index`) is used.

### 3.2 Input: HID → MIDI

| HID event | MIDI to DrivenByMoss |
| --- | --- |
| button down/up | template message with the `on`/`off` value (CC or note, per control) |
| Mk2 pad down/up with velocity | note on/off, with the template's velocity curve (`<velocitycurve>`) |
| Mk2 pad pressure | poly aftertouch, if the template defines `<polyat>` (to verify) |
| Mk2 encoders 1–8, main encoder | CC in the template's mode (`comp`: relative, two's complement as DrivenByMoss decodes it) |
| JAM main encoder | as above |
| JAM touch strip position / release | CC with the position (0..127 from 0..1023), and the touch as poly aftertouch (`CapTstA..H`) |
| JAM matrix | the template's note per pad (`BtnA1..H8`) |

### 3.3 Output: MIDI → LEDs and displays

| MIDI from DrivenByMoss | Device |
| --- | --- |
| CC/note value for a button control | button LED: 0 off, otherwise brightness or colour |
| CC/note value for an RGB control (JAM pads, groups and scenes; Mk2 pads and groups) | colour from the **NI MIDI-mode palette**. DrivenByMoss sends palette indices (`colors.rs`, e.g. red 6, white 78), and `palette.cpp` converts them: JAM to hue × 4 + level, Mk2 to RGB. The NI palette has to be derived (§6) |
| JAM `STRIP_SETUP` sysex (`F0 00 21 09 15 00 4D 50 …`, `jam/surface.rs`) | strip mode (single, dot, pan, dual) and colour |
| JAM strip CC (value feedback) | strip LED bar, drawn per the strip mode |
| Mk2 MCU display sysex (`McuDisplay`, `F0 00 00 66 14 12 …`) | text into the two 256×64 displays: 2 lines × 56 characters MCU layout, split across both displays, rendered with the bitmap font |
| anything else | ignored, logged once |

## 4. How DrivenByMoss reaches it

Both ways use the same core library. Only the transport differs.

### 4.1 In-process: virtual MIDI devices of the host (stage 2)

- `ni_host.h` is a C API:
  - `nihost_create(kind)` / `nihost_destroy`;
  - `nihost_send(h, bytes, n)`: MIDI from DrivenByMoss;
  - `nihost_poll(h, callback)`: MIDI to DrivenByMoss;
  - `nihost_is_connected(h)`.
- A host backend, such as the Reaper backend or the Zynthian Python wrapper, adds the devices to
  its `get_midi_inputs`/`get_midi_outputs` lists under NI's port names, with IDs above Reaper's
  range. It routes `send_midi_data` for those IDs to `nihost_send`, and feeds polled MIDI to
  `dbm_on_midi_event`.
- No extra process and no ALSA. The C ABI of DrivenByMoss does not change.

### 4.2 Standalone daemon `dbm-nihost` (stage 1)

- It creates ALSA sequencer ports named like NI's ports: `Maschine MK2 In`/`Out`,
  `Maschine Jam - 1 Input`/`Output`.
- Any application sees them: Reaper (enable them in the MIDI preferences), the Java DrivenByMoss
  in Bitwig, and Zynthian.
- The fastest way to test end to end, and it allows comparing the Rust and Java versions
  directly.
- Needs `libasound`. Runs as a user service, with access to hidraw through the udev rule.

**Recommendation: build the daemon first** (stage 1). It proves the translation with unchanged
hosts and works for the Java version too. Add the in-process route when Zynthian needs it.

## 5. Changes outside `support/linux`

- **Rust (done):** the Linux guard in `create()` of both Maschine factories is dropped. The
  discovery pairs have NI's port names plus the names of PipeWire's MIDI bridge, which Reaper
  shows.
- **Java DrivenByMoss (done, local branch `linux-maschine`):** the Linux check in
  `MaschineControllerSetup.init` and `MaschineJamControllerSetup.init` is removed, and the JAM
  has Linux discovery pairs.
- **Reaper backend:** nothing for stage 1. For stage 2, add the virtual device integration
  (§4.1) behind a CMake option.
- **Docs (done):** `STATUS.md` and `support/linux/ni_host/README.md`.

## 6. Open questions to settle first

1. **The NI MIDI-mode colour palette.**
   - Which RGB, or JAM hue and level, does each value 0..127 mean in MIDI mode?
   - Sources: the `<led>` settings in the templates, `MaschineColorManager` (Java, with the
     simulator's RGB values), and cabl's JAM palette.
   - Verify on the device: send each index and compare with NI's own behaviour where possible.
2. **Pad pressure:** does the Mk2 template send poly aftertouch, and does the DrivenByMoss
   Maschine code use it?
3. **Mk2 display text:** where exactly does NI's driver put the MCU text: line lengths, cell
   widths, which display? Match it closely enough that DrivenByMoss's layout (`mk/mode/*`) reads
   well. Consider later a "native" path that renders the graphic displays directly.
4. **Velocity curve** (`<velocitycurve>3`): which curve NI applies. Start with KICK's pressure
   mapping.
5. **Touch-strip feedback:** which strip modes `STRIP_SETUP` selects, and how NI draws each on
   the 11 LEDs.
6. **Mk3, Mikro Mk3, Plus, Studio:** these have different USB protocols and are not covered by
   KICK. Out of scope here.

## 7. Steps

- [x] **1. Skeleton.**
  - `support/linux/ni_host` with CMake.
  - Copy the KICK protocol classes (relicensed, namespace).
  - Port `hidraw_device` to std; a `nihost-probe` tool that logs decoded events.
  - Test with `jam_sim.py` and the real JAM.
- [x] **2. Template tables.**
  - `gen_template_map.py` and the generated `template_map.h`.
  - Test against the constants of the Rust surfaces.
- [x] **3. JAM input → MIDI.** Translator, with unit tests on synthetic reports.
- [x] **4. Daemon with ALSA ports.**
  - Test with `aseqdump`, then with the Rust DrivenByMoss in Reaper: buttons, matrix, strips,
    encoder.
- [x] **5. JAM output.**
  - The palette (§6.1), the matrix, group and scene colours, button LEDs.
  - `STRIP_SETUP` and the strip LEDs.
  - Compare with the Java version in Bitwig.
- [x] **6. Mk2 input → MIDI.** Pads with velocity, encoders, buttons.
- [x] **7. Mk2 output.** LEDs, pad and group colours, the MCU text on the displays with the
  bitmap font.
- [x] **8. Packaging.**
  - `packaging/`: the udev rule, a systemd user service (`dbm-nihost --virmidi`), the
    `snd-virmidi` setup for Bitwig. `cmake --install` installs them.
  - `ci/linux.sh` builds and tests the daemon with the crate, the wrapper and the Reaper
    extension; `.github/workflows/linux.yml` runs it once `rust/` is its own repository.
- [ ] **9. In-process route (§4.1)** for the Reaper backend and Zynthian.

## 7a. Beyond NI's MIDI mode (todo)

NI's driver cannot send some Mk2 controls in MIDI mode, so DrivenByMoss works around them (from
the DrivenByMoss manual, Maschine Mk2):

- SHIFT does not work; STEP LEFT (above REC) replaces both STOP and SHIFT
  (`shift_button` in `controller/maschine/core/surface.rs` returns Stop for the Mk2).
- Page left / right cannot be used; NAVIGATE and the pads are used instead.
- 4D encoder left/right and up/down: the two arrows below the encoder (with Shift for up/down).
- The Events button is STEP LEFT + Select/Events.

`dbm-nihost` reads the HID reports itself and sees all of these buttons. It could send them:
the real SHIFT as NI's Shift sysex (as on the JAM; the core already knows the Mk2's message,
`Maschine::Mk2.message_shift_down`), Page left/right and the encoder directions as additional
notes or CCs outside the template. The core then needs a Mk2 variant which uses them (e.g. a
flag that `dbm-nihost` is the host, so that SHIFT is the Shift button and STEP LEFT is STOP
again). This only works with `dbm-nihost`, not with NI's driver on Windows and macOS, so it has
to stay optional.

## 8. Testing

- **Unit tests** (no hardware): feed recorded or synthetic HID reports into the translators and
  compare the MIDI out. Feed MIDI in and compare the HID output reports. The KICK protocol
  classes already have defined report layouts to build these from.
- **uhid end-to-end** (root for `/dev/uhid`): `jam_sim.py` (and an Mk2 variant) makes a virtual
  device → `dbm-nihost` → ALSA → `aseqdump`, and the reverse direction with `aplaymidi`/`amidi`.
- **Hardware:** the real JAM and Mk2 with the Rust DrivenByMoss in Reaper. Where in doubt, compare
  with the Java DrivenByMoss through the same daemon.
