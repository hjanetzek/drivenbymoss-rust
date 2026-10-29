# dbm-nihost: NI Maschine Mk2 and JAM on Linux

On Windows and macOS, NI's driver turns the USB HID of a Maschine into MIDI ports. There is no
such driver on Linux. `dbm-nihost` does its job: it drives the **Maschine Mk2** and the
**Maschine JAM** over hidraw and creates ALSA sequencer ports with NI's names:

| Device | DAW input | DAW output |
| --- | --- | --- |
| Maschine Mk2 | `Maschine MK2 In` | `Maschine MK2 Out` |
| Maschine JAM | `Maschine Jam - 1 Input` | `Maschine Jam - 1 Output` |

The MIDI follows the DrivenByMoss NI Controller Editor templates (`resources/Maschine Mk2`,
`resources/Maschine Jam`). DrivenByMoss (the Rust port, or the Java version) therefore works with
these ports as with NI's driver. Background and design: `doc/NI_HOST_LINUX_PLAN.md`.

## Build and run

Needs CMake, a C++17 compiler and the ALSA headers (`alsa-lib`).

```
cmake -S support/linux/ni_host -B build-nihost
cmake --build build-nihost
build-nihost/dbm-nihost            # both devices; --mk2 / --jam for one, --log-midi to log the MIDI
```

The devices can be plugged in and out while it runs. Access to `/dev/hidraw*` needs the udev rule
`packaging/70-dbm-nihost.rules`.

## Install

```
cmake -S support/linux/ni_host -B build-nihost -D CMAKE_INSTALL_PREFIX=$HOME/.local
cmake --build build-nihost
cmake --install build-nihost
```

This installs:

- `bin/dbm-nihost`.
- `share/systemd/user/dbm-nihost.service`, a systemd user service that runs `dbm-nihost --virmidi`.
- `share/dbm-nihost/`: the udev rule and the `snd-virmidi` setup. These belong in `/etc` and need
  root:

```
sudo cp ~/.local/share/dbm-nihost/70-dbm-nihost.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
sudo cp ~/.local/share/dbm-nihost/modprobe-dbm-nihost.conf /etc/modprobe.d/dbm-nihost.conf        # Bitwig only
sudo cp ~/.local/share/dbm-nihost/modules-load-dbm-nihost.conf /etc/modules-load.d/dbm-nihost.conf
```

Then start the service, and on every login from then on:

```
systemctl --user daemon-reload
systemctl --user enable --now dbm-nihost
journalctl --user -u dbm-nihost -f              # the log
```

## Use in a DAW

### Reaper

Reaper opens the sequencer ports through PipeWire's MIDI bridge, named
`Midi-Bridge:Maschine MK2: In (capture)` and so on. The Rust port finds them by these names.

### Bitwig Studio

Bitwig only opens raw MIDI devices (`/dev/snd/midiC*D*`), not sequencer ports. The kernel's
virtual MIDI driver bridges the two:

- `snd-virmidi` with `midi_devs=2` creates two raw MIDI devices, each also a sequencer client
  (`Virtual Raw MIDI N-0`, `N-1`). The files in `packaging/` load it at boot; for a quick test,
  `sudo modprobe snd-virmidi midi_devs=2`.
- `dbm-nihost --virmidi` connects its ports to them: the 1st to the Mk2, the 2nd to the JAM (in this
  order, for the enabled devices). It checks every 2 seconds, so the module can also be loaded
  after the start.
- In Bitwig, select `Virtual Raw MIDI/1` (input and output) for the Mk2 and `Virtual Raw MIDI 2/1`
  for the JAM. Bitwig lists the 16 subdevices of each device; they all carry the same MIDI.
- Do not select the Mk2's own `Maschine Controller MK2 MIDI 1`, which disturbs the HID connection.
- The Java DrivenByMoss refuses the Maschine on Linux unless its Linux check is removed
  (`MaschineControllerSetup.init`, `MaschineJamControllerSetup.init`).

## Tools and tests

- `nihost-probe` prints the decoded HID events of both devices and lights a test pattern.
- `nihost-test` (also `ctest`) tests the translation with synthetic HID reports and MIDI.
- `tools/gen_template_map.py <repository root> src/template_map.h` regenerates the template
  tables from the NI templates. `tests/ni_templates.rs` in the Rust crate checks them against the
  Maschine surfaces.

## What is emulated

- **Input:** buttons, pads (Mk2 with velocity), encoders (relative, two's complement), the JAM
  matrix, touch strips with touch, and the JAM's Shift (as NI's sysex).
- **Output:** button LEDs, pad, group and scene colours (NI's palette, hue × 4 + level), the JAM
  strip modes from the `STRIP_SETUP` sysex, and the Mk2 displays (Mackie MCU text, 2 × 56
  characters split across both displays).
- **Not yet:** the Mk2 pad pressure as poly aftertouch, the JAM level meters and footswitch.

## Origin

The HID protocol classes come from KICK's `kick_controllers` module (same author, relicensed to
LGPL-3.0). Their tables, the font and part of the palette come from
[cabl](https://github.com/shaduzlabs/cabl), MIT license, see `LICENSE-cabl.txt`.
