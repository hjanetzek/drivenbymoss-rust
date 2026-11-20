# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Akai APC40 mkII in "Ableton Live mode" as DrivenByMoss uses it (Rust core
src/controller/akai/apc/, Java controller/akai/apc).

- One duplex port "APC40 mkII MIDI 1" (client "APC40 mkII"; the core's discovery names are
  "APC40 mkII", "APC40 mkII MIDI <n>" and "<n>- APC40 mkII").
- At its start the core sends the mode SysEx ``F0 47 7F 29 60 00 04 41 08 02 01 F7`` (0x41: Ableton
  Live mode, in which the knobs send absolute values and the host drives their LED rings).
- Buttons are notes (on 127, off: note off), the track buttons on the MIDI channel of the track;
  the footswitch is CC 0x40.
- The clip pads (notes 0..39 on channel 1, 0 bottom left) and the scene buttons (0x52..0x56) have
  RGB LEDs: velocity = palette index; channels 2..6 one-shot, 7..11 pulsing, 12..16 blinking (the
  core uses 11 for slow, 13 for fast blinking). The other buttons have single LEDs (0 off, 1 on,
  2 blinking); the crossfader assignment 0x42: 1 A (yellow), 2 B (orange).
- Faders CC 7 on the track's channel, master CC 14, crossfader CC 15; the track knobs CC 0x30..0x37
  and the device knobs CC 0x10..0x17 absolute; their rings: the value with the same CC, the mode with
  CC 0x38..0x3F / 0x18..0x1F (0 off, 1 single, 2 volume, 3 pan). Tempo (CC 0x0D) and cue level
  (CC 0x2F) are relative.
"""

from .base import Control, GridDevice

MODE_SYSEX_PREFIX = bytes([0xF0, 0x47, 0x7F, 0x29, 0x60])
MODES = {0x40: "Generic mode", 0x41: "Ableton Live mode", 0x42: "Alternate Ableton Live mode"}
ANIMATION_NOTES = (1 / 24, 1 / 16, 1 / 8, 1 / 4, 1 / 2)

# Single LED colours
AMBER, BLUE, RED, YELLOW, GREEN, WHITE = (255, 150, 20), (40, 110, 255), (255, 30, 20), (255, 220, 30), (40, 230, 60), (235, 235, 230)


class Apc40Mk2(GridDevice):
    MODEL = "apc40mk2"
    TITLE = "APC40 mkII Simulator"
    SURFACE = (300.0, 196.0)
    CLIENT = "APC40 mkII"
    PORTS = {"midi": "APC40 mkII MIDI 1"}
    UUID = "14787D10-35DE-11E4-8C21-0800200C9A66"

    def build_controls(self) -> list[Control]:
        c: list[Control] = []

        def button(label, number, x, y, w=18.0, h=7.0, channel=0, led="single", colour=AMBER, const=""):
            c.append(Control("button", label, x, y, w, h, "note", channel, number, led, colour, const=const))

        for i in range(8):
            x = 6 + i * 20
            c.append(Control("encoder", "", x + 2, 4, 14, 14, "cc", 0, 0x30 + i, "none", ring=(0x30 + i, 0x38 + i),
                             const="APC_KNOB_TRACK_KNOB_1"))
            for row in range(5):
                c.append(Control("pad", "", x, 24 + (4 - row) * 13, 18, 11, "note", 0, row * 8 + i, "rgb"))
            button("■", 0x34, x, 90, channel=i, colour=GREEN, const="APC_BUTTON_CLIP_STOP")
            button(f"{i + 1}", 0x33, x, 100, channel=i, colour=AMBER, const="APC_BUTTON_TRACK_SELECTION")
            button("A|B", 0x42, x, 110, h=6, channel=i, led="ab", const="APC_BUTTON_A_B")
            button(f"{i + 1}", 0x32, x, 118, channel=i, colour=YELLOW, const="APC_BUTTON_ACTIVATOR")
            button("S", 0x31, x, 127, channel=i, colour=BLUE, const="APC_BUTTON_SOLO")
            button("●", 0x30, x, 136, channel=i, colour=RED, const="APC_BUTTON_RECORD_ARM")
            c.append(Control("fader", "", x + 4, 146, 10, 46, "cc", i, 0x07, "none", const="APC_KNOB_TRACK_LEVEL"))
        for row in range(5):
            button("▶", 0x52 + row, 168, 24 + row * 13, 18, 11, led="rgb", const="APC_BUTTON_SCENE_LAUNCH_1")
        button("Stop All", 0x51, 168, 90, const="APC_BUTTON_STOP_ALL_CLIPS")
        button("Master", 0x50, 168, 100, const="APC_BUTTON_MASTER")
        c.append(Control("fader", "Master", 172, 146, 10, 46, "cc", 0, 0x0E, "none", const="APC_KNOB_MASTER_LEVEL"))

        # Device section
        for i in range(8):
            c.append(Control("encoder", "", 196 + (i % 4) * 25, 4 + (i // 4) * 22, 17, 17, "cc", 0, 0x10 + i, "none",
                             ring=(0x10 + i, 0x18 + i), const="APC_KNOB_DEVICE_KNOB_1"))
        for i, (label, number, const) in enumerate((("◀ Device", 0x3A, "APC_BUTTON_CLIP_TRACK"),
                                                    ("Device ▶", 0x3B, "APC_BUTTON_DEVICE_ON_OFF"),
                                                    ("◀ Bank", 0x3C, "APC_BUTTON_DEVICE_LEFT"),
                                                    ("Bank ▶", 0x3D, "APC_BUTTON_DEVICE_RIGHT"),
                                                    ("Dev. On/Off", 0x3E, "APC_BUTTON_DETAIL_VIEW"),
                                                    ("Dev. Lock", 0x3F, "APC_BUTTON_REC_QUANT"),
                                                    ("Clip/Dev. View", 0x40, "APC_BUTTON_MIDI_OVERDUB"),
                                                    ("Detail View", 0x41, "APC_BUTTON_METRONOME"))):
            button(label, number, 194 + (i % 4) * 25, 48 + (i // 4) * 10, 23, 8, const=const)
        for i, (label, number, const) in enumerate((("Pan", 0x57, "APC_BUTTON_PAN"), ("Sends", 0x58, "APC_BUTTON_SEND_A"),
                                                    ("User", 0x59, "APC_BUTTON_SEND_B"), ("Metronome", 0x5A, "APC_BUTTON_SEND_C"))):
            button(label, number, 194 + i * 25, 72, 23, 8, const=const)
        button("Play", 0x5B, 194, 86, 23, 9, colour=GREEN, const="APC_BUTTON_PLAY")
        button("Record", 0x5D, 219, 86, 23, 9, colour=RED, const="APC_BUTTON_RECORD")
        button("Session", 0x66, 244, 86, 23, 9, colour=GREEN, const="APC_BUTTON_SESSION")
        c.append(Control("encoder", "Tempo", 272, 84, 18, 18, "cc", 0, 0x0D, "none", relative=True, const="APC_KNOB_TEMPO"))
        button("Tap Tempo", 0x63, 194, 100, 23, 8, led="none", const="APC_BUTTON_TAP_TEMPO")
        button("Nudge -", 0x64, 219, 100, 23, 8, led="none", const="APC_BUTTON_NUDGE_PLUS")
        button("Nudge +", 0x65, 244, 100, 23, 8, led="none", const="APC_BUTTON_NUDGE_MINUS")
        c.append(Control("encoder", "Cue", 196, 116, 16, 16, "cc", 0, 0x2F, "none", relative=True, const="APC_KNOB_CUE_LEVEL"))
        button("Shift", 0x62, 194, 140, 23, 8, led="none", const="APC_BUTTON_SHIFT")
        button("Bank", 0x67, 219, 140, 23, 8, const="APC_BUTTON_BANK")
        button("▲", 0x5E, 254, 114, 14, 8, led="none", const="APC_BUTTON_UP")
        button("◀", 0x61, 238, 124, 14, 8, led="none", const="APC_BUTTON_LEFT")
        button("▶", 0x60, 270, 124, 14, 8, led="none", const="APC_BUTTON_RIGHT")
        button("▼", 0x5F, 254, 134, 14, 8, led="none", const="APC_BUTTON_DOWN")
        c.append(Control("button", "Foot", 270, 140, 20, 8, "cc", 0, 0x40, "none", const="APC_FOOTSWITCH_1"))
        c.append(Control("fader", "Crossfader", 200, 172, 90, 12, "cc", 0, 0x0F, "none", const="APC_KNOB_CROSSFADER"))
        return c

    def led_for(self, kind, channel, number):
        if kind == "note":
            # The pads and scene buttons: the animations on the channels 2..16
            if number < 40 or 0x52 <= number <= 0x56:
                key = self.by_message.get(("note", 0, number))
                return (key, channel) if key is not None else None
            key = self.by_message.get(("note", channel, number))
            return (key, 0) if key is not None else None
        key = self.by_message.get((kind, channel, number))
        return (key, 0) if key is not None else None

    def animation(self, channel):
        whole = 4 * 60.0 / self.bpm
        group, index = divmod(channel - 1, 5)
        return (("oneshot", "pulse", "blink")[min(group, 2)], ANIMATION_NOTES[index] * whole)

    def handle_sysex(self, msg, port):
        if msg.startswith(MODE_SYSEX_PREFIX) and len(msg) >= 12:
            self.mode = MODES.get(msg[7], f"mode 0x{msg[7]:02X}")
            self.changed("settings")
            return f"mode: {self.mode}"
        return ""

    def identity_reply(self):
        # Short form (1 byte manufacturer ID 0x47): family 29 00, member 19 00, version
        return bytes([0xF0, 0x7E, 0x00, 0x06, 0x02, 0x47, 0x29, 0x00, 0x19, 0x00, 0x00, 0x01, 0x00, 0x00, 0xF7])

    def status_text(self):
        return self.mode or "mode not set (the core sends the Ableton Live mode SysEx)"

