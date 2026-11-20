# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The controls of the Maschine Mk2 and the Maschine JAM: template ids, labels and positions.

Every control has a ``key`` (its index): the QML view identifies controls by it (the ``cc`` and
``note`` of the control's dict), the device model maps it to the template control
(``template``, an id of the DrivenByMoss NI template, see templates.py). Positions are in surface
units (about millimetres, origin top left). The arrangement follows the hardware loosely; it is
an own drawing and may diverge where it helps (the simulators are also the base of a touch
controller).
"""

from dataclasses import dataclass

MODELS = ("mk2", "jam", "mk3", "mikromk3")


@dataclass
class Control:
    kind: str  # "pad", "button", "encoder", "display", "strip", "meter"
    label: str
    x: float
    y: float
    w: float
    h: float
    template: str = ""
    """The id of the template control which the control sends (pads, buttons, encoders, strips)."""
    touch: str = ""
    """The template id of the touch message (JAM knob and strips)."""
    rgb: bool = False
    """The LED shows NI's palette colour (pads, groups, scenes), else a brightness."""
    index: int = 0
    """The display (0 left, 1 right), the strip or meter number."""
    key: int = -1

    def as_dict(self) -> dict:
        return {"kind": self.kind, "label": self.label, "x": self.x, "y": self.y, "w": self.w, "h": self.h,
                "cc": self.key, "note": self.key, "rgb": self.rgb, "index": self.index}


SURFACE = {"mk2": (322.0, 200.0), "jam": (262.0, 252.0), "mk3": (350.0, 220.0), "mikromk3": (262.0, 168.0)}

# The client and port names of NI's driver and dbm-nihost: (client, DAW input, DAW output)
PORT_NAMES = {
    "mk2": ("Maschine MK2", "Maschine MK2 In", "Maschine MK2 Out"),
    "jam": ("Maschine Jam - 1", "Maschine Jam - 1 Input", "Maschine Jam - 1 Output"),
    # NI's driver (Windows) names the in- and output of the Mk3 and the Mikro Mk3 alike; the core's
    # discovery uses these names on Linux too. There is no dbm-nihost for them, the simulator
    # creates one duplex port.
    "mk3": ("Maschine MK3", "Maschine MK3 Ctrl MIDI", "Maschine MK3 Ctrl MIDI"),
    "mikromk3": ("Maschine Mikro MK3", "Maschine Mikro MK3", "Maschine Mikro MK3"),
}


def midi_names(model: str, suffix: str = "") -> tuple[str, str, str]:
    """The ALSA names; a suffix (e.g. " (sim)") is appended to all three, so the simulator can
    run next to dbm-nihost, which owns the original names."""
    return tuple(name + suffix for name in PORT_NAMES[model])


def _mk2() -> list[Control]:
    c: list[Control] = []

    def button(label, template, x, y, w=20.0, h=7.0, rgb=False):
        c.append(Control("button", label, x, y, w, h, template=template, rgb=rgb))

    # Left column
    for i, (label, template) in enumerate((("Control", "Control"), ("Step", "Step"), ("Browse", "Browse"),
                                          ("Sampling", "Sampling"), ("All", "All"), ("Auto Write", "AutoWrite"))):
        button(label, template, 4, 4 + i * 10.5)
    # The 8 buttons above the displays, the displays, the 8 encoders
    for i in range(8):
        button(f"{i + 1}", f"Button{i + 1}", 32 + i * 30.5, 4, 26, 6)
        c.append(Control("encoder", "", 32 + i * 30.5 + 6, 48, 14, 14, template=f"Knob{i + 1}"))
    c.append(Control("display", "", 32, 12, 118, 32, index=0))
    c.append(Control("display", "", 154.5, 12, 118, 32, index=1))
    # Master section
    button("Volume", "Volume", 280, 4, 12, 7)
    button("Swing", "Swing", 294, 4, 12, 7)
    button("Tempo", "Tempo", 308, 4, 12, 7)
    button("◂", "MasterL", 280, 14, 12, 7)
    button("▸", "MasterR", 294, 14, 12, 7)
    button("Enter", "Enter", 308, 14, 12, 7)
    button("Note Rep.", "NoteRep", 280, 24, 12, 9)
    c.append(Control("encoder", "Master", 296, 24, 22, 22, template="Dial"))
    button("Push", "Push", 280, 38, 12, 7)
    # Groups and transport
    for i in range(8):
        button(chr(ord("A") + i), "Group" + chr(ord("A") + i), 4 + (i % 4) * 18, 74 + (i // 4) * 15, 16, 12, rgb=True)
    for i, (label, template) in enumerate((("Restart", "Restart"), ("◂", "StepL"), ("▸", "StepR"), ("Grid", "Grid"),
                                          ("Play", "Play"), ("Rec", "Rec"), ("Erase", "Erase"))):
        button(label, template, 4 + (i % 4) * 18, 112 + (i // 4) * 15, 16, 12)
    # Shift: not in the template, sent as the DrivenByMoss SysEx 12
    button("Shift", "", 4 + 3 * 18, 112 + 15, 16, 12)
    # The column left of the pads
    for i, (label, template) in enumerate((("Scene", "Scene"), ("Pattern", "Pattern"), ("Pad Mode", "PadMode"),
                                          ("Navigate", "Navigate"), ("Duplicate", "Duplicate"), ("Select", "Select"),
                                          ("Solo", "Solo"), ("Mute", "Mute"))):
        button(label, template, 84, 74 + i * 15.5, 26, 12)
    # Pads 1..16 from the bottom left
    for n in range(16):
        row, col = divmod(n, 4)
        c.append(Control("pad", f"{n + 1}", 126 + col * 47, 74 + (3 - row) * 31, 43, 28, template=f"Pad{n + 1}", rgb=True))
    return c


def _jam() -> list[Control]:
    c: list[Control] = []

    def button(label, template, x, y, w=30.0, h=9.0, rgb=False):
        c.append(Control("button", label, x, y, w, h, template=template, rgb=rgb))

    x0, y0, pitch = 40.0, 24.0, 20.0
    button("Song", "BtnArrange", 4, 8, 30, 10)
    for i in range(8):
        button(f"{i + 1}", f"BtnScene{i + 1}", x0 + i * pitch, 8, 18, 10, rgb=True)
        button(chr(ord("A") + i), f"BtnGroup{chr(ord('A') + i)}", x0 + i * pitch, y0 + 8 * pitch + 2, 18, 10, rgb=True)
        c.append(Control("strip", "", x0 + i * pitch + 3, y0 + 8 * pitch + 16, 12, 50,
                         template=f"Tst{chr(ord('A') + i)}", touch=f"CapTst{chr(ord('A') + i)}", rgb=True, index=i))
        for row in range(8):
            c.append(Control("pad", "", x0 + i * pitch, y0 + row * pitch, 18, 18,
                             template=f"Btn{chr(ord('A') + i)}{row + 1}", rgb=True))
    # Left column
    for i, (label, template) in enumerate((("Step", "BtnStep"), ("Pad Mode", "BtnPadMode"), ("Clear", "BtnClear"),
                                          ("Duplicate", "BtnDuplicate"))):
        button(label, template, 4, 26 + i * 12)
    button("▴", "BtnDpad1", 14, 76, 10, 8)
    button("◂", "BtnDpad4", 4, 85, 10, 8)
    button("▸", "BtnDpad2", 24, 85, 10, 8)
    button("▾", "BtnDpad3", 14, 94, 10, 8)
    button("Note Rep.", "BtnArpRepeat", 4, 108)
    for i, (label, template) in enumerate((("Play", "BtnPlay"), ("Rec", "BtnRecord"))):
        button(label, template, 4, 150 + i * 12)
    button("◂", "BtnArrowLeft", 4, 174, 14, 9)
    button("▸", "BtnArrowRight", 20, 174, 14, 9)
    # Shift: not in the template, sent as NI's SysEx
    button("Shift", "", 4, 230, 30, 12)
    # Right column with the level meters (CC 64/65)
    for i, (label, template) in enumerate((("Master", "BtnMst"), ("Group", "BtnGrp"), ("In", "BtnIn1"),
                                          ("Cue", "BtnCue"), ("Browse", "BtnBrowse"))):
        button(label, template, 204, 8 + i * 11, 34, 9)
    c.append(Control("meter", "L", 242, 8, 6, 51, index=0))
    c.append(Control("meter", "R", 250, 8, 6, 51, index=1))
    c.append(Control("encoder", "", 212, 66, 26, 26, template="EncBrowse", touch="CapBrowse"))
    button("Push", "PshBrowse", 240, 72, 18, 9)
    for i, (label, template) in enumerate((("Macro", "BtnMacro"), ("Level", "BtnLevel"), ("Aux", "BtnAux"),
                                          ("Control", "BtnControl"), ("Auto", "BtnAuto"))):
        button(label, template, 204, 100 + i * 11, 52, 9)
    for i, ((l1, t1), (l2, t2)) in enumerate(((("Perform", "BtnPerform"), ("Notes", "BtnVariation")),
                                              (("Lock", "BtnLock"), ("Tune", "BtnTune")),
                                              (("Swing", "BtnSwing"), ("Tempo", "BtnTempo")),
                                              (("Grid", "BtnGrid"), ("Solo", "BtnSolo")),
                                              (("Mute", "BtnMute"), ("Select", "BtnSelect")))):
        button(l1, t1, 204, 160 + i * 17, 25, 14)
        button(l2, t2, 231, 160 + i * 17, 25, 14)
    return c


def _pads_and_column(c: list[Control], button, column_x, column_w, pad_x, pad_y, pad_w, pad_h, pitch_x, pitch_y, step):
    """The column left of the pads (Scene .. Mute) and the 16 pads (Pad1 at the bottom left)."""
    for i, (label, template) in enumerate((("Scene", "Scene"), ("Pattern", "Pattern"), ("Events", "Events"),
                                          ("Variation", "Variation"), ("Duplicate", "Duplicate"), ("Select", "Select"),
                                          ("Solo", "Solo"), ("Mute", "Mute"))):
        button(label, template, column_x, pad_y + i * step, column_w, step - 3)
    for n in range(16):
        row, col = divmod(n, 4)
        c.append(Control("pad", f"{n + 1}", pad_x + col * pitch_x, pad_y + (3 - row) * pitch_y, pad_w, pad_h,
                         template=f"Pad{n + 1}", rgb=True))


PAD_MODE_ROW = (("Fixed Vel", "FixedVel"), ("Pad Mode", "PadMode"), ("Keyboard", "Keyboard"), ("Chords", "Chords"),
                ("Step", "Step"))


def _mk3() -> list[Control]:
    c: list[Control] = []

    def button(label, template, x, y, w=26.0, h=8.0, rgb=False):
        c.append(Control("button", label, x, y, w, h, template=template, rgb=rgb))

    # Top left: the mode buttons and the file buttons
    for i, ((l1, t1), (l2, t2)) in enumerate(((("Channel", "Channel"), ("Plug-in", "Plugin")),
                                              (("Arranger", "Arranger"), ("Mixer", "Mixer")),
                                              (("Browser", "Browser"), ("Sampling", "Sampling")),
                                              (("◀", "Left"), ("▶", "Right")),
                                              (("File", "File"), ("Settings", "Setting")))):
        button(l1, t1, 4, 4 + i * 10)
        button(l2, t2, 32, 4 + i * 10)
    # The 8 buttons above the displays, the displays, the 8 touch-sensitive knobs
    for i in range(8):
        button(f"{i + 1}", f"Button{i + 1}", 64 + i * 30.5, 4, 26, 6)
        c.append(Control("encoder", "", 64 + i * 30.5 + 6, 48, 14, 14, template=f"Knob{i + 1}", touch=f"KnobCap{i + 1}"))
    c.append(Control("display", "", 64, 12, 118, 32, index=0))
    c.append(Control("display", "", 186.5, 12, 118, 32, index=1))
    # The 4-D encoder: turn (and touch), push, tilt in four directions
    button("▲", "EncoderUp", 319, 2, 14, 6)
    button("◀", "EncoderLeft", 309, 9, 8, 14)
    c.append(Control("encoder", "", 319, 9, 14, 14, template="Encoder", touch="EncoderCap"))
    button("▶", "EncoderRight", 335, 9, 8, 14)
    button("▼", "EncoderDown", 319, 24, 14, 6)
    button("Push", "EncoderPush", 319, 32, 14, 6)
    for i, (label, template) in enumerate((("Volume", "Volume"), ("Swing", "Swing"), ("Tempo", "Tempo"))):
        button(label, template, 304 + i * 15, 42, 14, 8)
    # Left: performance, groups, transport (Shift: NI's SysEx, not in the template)
    for i, ((l1, t1), (l2, t2)) in enumerate(((("Macro", "Macro"), ("Note Rep.", "NoteRep")),
                                              (("Auto", "Auto"), ("Lock", "Lock")),
                                              (("Pitch", "Pitch"), ("Mod", "Mod")),
                                              (("Perform", "Perform"), ("Notes", "Notes")))):
        button(l1, t1, 4, 56 + i * 10)
        button(l2, t2, 32, 56 + i * 10)
    for i in range(8):
        button(chr(ord("A") + i), "Group" + chr(ord("A") + i), 4 + (i % 4) * 14.5, 100 + (i // 4) * 14, 13, 12, rgb=True)
    for i, ((l1, t1), (l2, t2)) in enumerate(((("Restart", "Restart"), ("Erase", "Erase")),
                                              (("Tap", "Tap"), ("Follow", "Follow")),
                                              (("Play", "Play"), ("Rec", "Rec")),
                                              (("Stop", "Stop"), ("Shift", "")))):
        button(l1, t1, 4, 132 + i * 14, 26, 12)
        button(l2, t2, 32, 132 + i * 14, 26, 12)
    # The touch strip and the pad mode buttons above the pads
    c.append(Control("strip", "", 110, 68, 96, 8, template="Touchstrip", touch="TouchstripCap", index=0))
    for i, (label, template) in enumerate(PAD_MODE_ROW):
        button(label, template, 212 + i * 27, 67, 25, 10)
    _pads_and_column(c, button, 66, 38, 110, 82, 56, 31, 60, 34, 16.5)
    return c


def _mikro_mk3() -> list[Control]:
    c: list[Control] = []

    def button(label, template, x, y, w=24.0, h=8.0, rgb=False):
        c.append(Control("button", label, x, y, w, h, template=template, rgb=rgb))

    for i, (label, template) in enumerate((("Maschine", "Maschine"), ("★", "Star"), ("Browser", "Browser"),
                                          ("Volume", "Volume"), ("Swing", "Swing"), ("Tempo", "Tempo"),
                                          ("Plug-in", "Plugin"), ("Sampling", "Sampling"))):
        button(label, template, 4 + (i % 3) * 26, 4 + (i // 3) * 10)
    # The navigation encoder: turn (and touch), push
    c.append(Control("encoder", "", 88, 4, 20, 20, template="Navigation", touch="Navigation Touch"))
    button("Push", "Navigation Push", 86, 27, 24, 6)
    c.append(Control("strip", "", 4, 40, 104, 8, template="Touchstrip", touch="TouchstripCap", index=0))
    for i, (label, template) in enumerate((("Group", "Group"), ("Auto", "Auto"), ("Lock", "Lock"), ("Note Rep.", "NoteRep"),
                                          ("Pitch", "Pitch"), ("Mod", "Mod"), ("Perform", "Perform"), ("Notes", "Notes"))):
        button(label, template, 4 + (i % 4) * 26.5, 54 + (i // 4) * 12, 24, 10)
    for i, (label, template) in enumerate((("Restart", "Restart"), ("Erase", "Erase"), ("Tap", "Tap"), ("Follow", "Follow"),
                                          ("Play", "Play"), ("Rec", "Rec"), ("Stop", "Stop"))):
        button(label, template, 4 + (i % 4) * 26.5, 86 + (i // 4) * 15, 24, 12)
    for i, (label, template) in enumerate(PAD_MODE_ROW):
        button(label, template, 152 + i * 21.5, 4, 19.5, 10)
    _pads_and_column(c, button, 116, 32, 152, 20, 24, 33, 26.5, 36, 18)
    return c


def build_controls(model: str) -> list[Control]:
    if model not in MODELS:
        raise ValueError(f"unknown model {model!r}")
    controls = {"mk2": _mk2, "jam": _jam, "mk3": _mk3, "mikromk3": _mikro_mk3}[model]()
    for key, control in enumerate(controls):
        control.key = key
    return controls
