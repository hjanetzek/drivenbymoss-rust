# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The controls of the Maschine Mk2 and the Maschine JAM: template ids, labels and positions.

Every control has a ``key`` (its index): the QML view identifies controls by it (the ``cc`` and
``note`` of the control's dict), the device model maps it to the template control
(``template``, an id of the DrivenByMoss NI template, see templates.py). Positions are in surface
units (about millimetres, origin top left). The JAM and the Mk2 are placed from top view photos of
the devices; the Mk3 and the Mikro Mk3 follow the hardware loosely (own drawings, which may diverge
where it helps: the simulators are also the base of a touch controller).
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
    help_side: str = ""
    """The side of the panel for the control's help box (empty: the nearest, sim_common.help)."""
    help_label: str = ""
    """The name of the control in its help box (empty: the label, or the core's label)."""

    def as_dict(self) -> dict:
        return {"kind": self.kind, "label": self.label, "x": self.x, "y": self.y, "w": self.w, "h": self.h,
                "cc": self.key, "note": self.key, "rgb": self.rgb, "index": self.index}


SURFACE = {"mk2": (321.0, 262.0), "jam": (370.0, 312.0), "mk3": (350.0, 220.0), "mikromk3": (262.0, 168.0)}

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
    """The Mk2 as in a top view photo of the device (1864 × 1521 px, about 0.172 mm per pixel: the
    device is 32 cm wide; the logo strip above the buttons cut off): `px(cx, cy, w, h)` places a
    control by its centre and size in the photo's pixels. The controls are created in the same
    order as before (their keys); the arrows under BROWSE (not in the template, they send nothing
    in the MIDI mode) come last."""
    c: list[Control] = []
    scale = 0.172

    def px(cx, cy, w, h):
        return ((cx - w / 2) * scale, (cy - h / 2) * scale, w * scale, h * scale)

    def button(label, template, rect, rgb=False, help_label="", help_side=""):
        c.append(Control("button", label, *rect, template=template, rgb=rgb, help_label=help_label, help_side=help_side))

    left, right = 164, 311.5
    # The upper left block: Control, Step (their help boxes above the panel) / Browse, Sampling /
    # (the arrows) / All, Auto
    for (label, template), (x, y, h) in zip((("Control", "Control"), ("Step", "Step"), ("Browse", "Browse"), ("Sampling", "Sampling"),
                                             ("All", "All"), ("Auto", "AutoWrite")),
                                            ((left, 86, 48), (right, 86, 48), (left, 220.5, 69), (right, 220.5, 69), (left, 469.5, 45),
                                             (right, 469.5, 45))):
        button(label, template, px(x, y, 111, h), help_side="top" if y < 100 else "")
    # The 8 buttons above the displays, the displays, the 8 knobs under them (in the same columns)
    columns = (492, 666, 839.5, 1012, 1185, 1359, 1533.5, 1707.5)
    for i, x in enumerate(columns):
        button(f"{i + 1}", f"Button{i + 1}", px(x, 86.5, 110, 47), help_label=f"Display button {i + 1}", help_side="top")
        c.append(Control("encoder", "", *px(x, 467, 102, 102), template=f"Knob{i + 1}", help_label=f"Knob {i + 1}"))
    c.append(Control("display", "", *px(752, 275, 610, 170), index=0))
    c.append(Control("display", "", *px(1447, 275, 610, 170), index=1))
    # MASTER: Volume, Swing, Tempo in a column; the encoder (its push under it), Note Repeat;
    # ◂ ▸ Enter in the row under the encoder
    button("Volume", "Volume", px(left, 671.5, 111, 43))
    button("Swing", "Swing", px(left, 753.5, 111, 42))
    button("Tempo", "Tempo", px(left, 836, 111, 42))
    button("◂", "MasterL", px(right, 836, 111, 42), help_label="Encoder ◂")
    button("▸", "MasterR", px(459, 836, 110, 42), help_label="Encoder ▸")
    button("Enter", "Enter", px(606.5, 836, 109, 42))
    button("Note Repeat", "NoteRep", px(606.5, 689, 109, 76))
    c.append(Control("encoder", "", *px(385, 706, 156, 156), template="Dial", help_label="Encoder"))
    button("Push", "Push", px(385, 800, 90, 20), help_label="Encoder push")
    # GROUPS: A-D, E-H
    for i in range(8):
        button(chr(ord("A") + i), "Group" + chr(ord("A") + i), px((left, 311, 459, 607)[i % 4], (1023.5, 1141.5)[i // 4], 106, 78), rgb=True,
               help_label="Group " + chr(ord("A") + i))
    # TRANSPORT: Restart, ◂, ▸, Grid / Play, Rec, Erase, Shift (the help boxes below the panel)
    for i, (label, template, help_label) in enumerate((("Restart", "Restart", ""), ("◂", "StepL", "◂ (Stop)"), ("▸", "StepR", "▸ (Tap)"),
                                                      ("Grid", "Grid", ""), ("Play", "Play", ""), ("Rec", "Rec", ""), ("Erase", "Erase", ""))):
        x = (163.5, 311.5, 459, 606.5)[i % 4]
        button(label, template, px(x, 1329.5, 111, 47) if i < 4 else px(x, 1424, 109, 70), help_label=help_label, help_side="bottom")
    # Shift: not in the template, sent as the DrivenByMoss SysEx 12
    button("Shift", "", px(606.5, 1424, 109, 70), help_side="bottom")
    # The column left of the pads (the help boxes right of the panel: on the left they would cross
    # the lines of the master section and the groups)
    for i, (label, template) in enumerate((("Scene", "Scene"), ("Pattern", "Pattern"), ("Pad Mode", "PadMode"),
                                          ("Navigate", "Navigate"), ("Duplicate", "Duplicate"), ("Select", "Select"),
                                          ("Solo", "Solo"), ("Mute", "Mute"))):
        button(label, template, px(806.5, (683.5, 788.5, 894.5, 1000, 1105.5, 1212, 1318, 1424)[i], 107, 66), help_side="right")
    # PADS: 1..16 from the bottom left, square
    for n in range(16):
        row, col = divmod(n, 4)
        c.append(Control("pad", f"{n + 1}", *px((1038, 1250, 1461, 1672.5)[col], (1370, 1157.5, 947.5, 734)[row], 165, 165),
                         template=f"Pad{n + 1}", rgb=True))
    # The arrows under BROWSE: they send nothing in the MIDI mode (dbm-nihost: not in the template)
    button("◂", "", px(left, 326.5, 111, 69))
    button("▸", "", px(right, 326.5, 111, 69))
    return c


def _jam() -> list[Control]:
    """The JAM as in a top view photo of the device (1400 px wide, about 0.264 mm per pixel,
    the logo strip cut off): `px(cx, cy, w, h)` places a control by its centre and size in the
    photo's pixels. The controls are created in the same order as before (their keys)."""
    c: list[Control] = []
    scale, top = 0.264, 55.0

    def px(cx, cy, w, h):
        return ((cx - w / 2) * scale, (cy - top - h / 2) * scale, w * scale, h * scale)

    def button(label, template, rect, rgb=False, help_label="", help_side=""):
        c.append(Control("button", label, *rect, template=template, rgb=rgb, help_label=help_label, help_side=help_side))

    columns = [262 + i * 125.7 for i in range(8)]
    rows = [187 + r * 64 for r in range(8)]
    button("Song", "BtnArrange", px(83, 97, 92, 40), help_side="top")
    for i in range(8):
        letter = chr(ord("A") + i)
        button(f"{i + 1}", f"BtnScene{i + 1}", px(columns[i], 98, 94, 40), rgb=True, help_label=f"Scene {i + 1}")
        button(letter, f"BtnGroup{letter}", px(columns[i], 727, 94, 52), rgb=True, help_label=f"Group {letter}")
        # The strip and its LED column on its right
        c.append(Control("strip", "", *px(columns[i] + 8, 949, 82, 282), template=f"Tst{letter}", touch=f"CapTst{letter}", rgb=True, index=i))
        for row in range(8):
            c.append(Control("pad", "", *px(columns[i], rows[row], 94, 50), template=f"Btn{letter}{row + 1}", rgb=True))
    # The upper left column, the d-pad, Note Repeat on the group row
    for (label, template), y in zip((("Step", "BtnStep"), ("Pad Mode", "BtnPadMode"), ("Clear", "BtnClear"), ("Duplicate", "BtnDuplicate")),
                                    (184, 249, 313, 378)):
        button(label, template, px(83, y, 92, 42))
    for label, template, x, y, name in (("▴", "BtnDpad1", 83, 552, "Up"), ("◂", "BtnDpad4", 41, 596, "Left"), ("▸", "BtnDpad2", 125, 596, "Right"),
                                        ("▾", "BtnDpad3", 83, 640, "Down")):
        button(label, template, px(x, y, 40, 40), help_label=name)
    button("Note Repeat", "BtnArpRepeat", px(83, 729, 92, 52))
    # The bottom row under the strips (Play .. Mute in the columns)
    button("Play", "BtnPlay", px(columns[0], 1170, 94, 52))
    button("Rec", "BtnRecord", px(columns[1], 1170, 94, 52))
    button("◂", "BtnArrowLeft", px(columns[2], 1170, 94, 52), help_label="Page ◂")
    button("▸", "BtnArrowRight", px(columns[3], 1170, 94, 52), help_label="Page ▸")
    # Shift: not in the template, sent as NI's SysEx
    button("Shift", "", px(85, 1172, 92, 50))
    # The upper right column: the level meters (CC 64/65), MST GRP, IN 1 and the headphones
    button("MST", "BtnMst", px(1292, 372, 44, 36), help_label="MST")
    button("GRP", "BtnGrp", px(1347, 372, 44, 36), help_label="GRP")
    button("IN 1", "BtnIn1", px(1292, 446, 44, 36), help_label="IN 1")
    button("🎧", "BtnCue", px(1347, 446, 44, 36), help_label="Headphones")
    button("Browse", "BtnBrowse", px(1320, 728, 92, 50))
    c.append(Control("meter", "L", *px(1289, 230, 14, 190), index=0))
    c.append(Control("meter", "R", *px(1343, 230, 14, 190), index=1))
    c.append(Control("encoder", "", *px(1320, 590, 96, 96), template="EncBrowse", touch="CapBrowse", help_label="Encoder"))
    button("Push", "PshBrowse", px(1320, 668, 56, 24), help_label="Encoder push")
    # The block left of the strips, the block right of them
    for (label, template), y in zip((("Macro", "BtnMacro"), ("Level", "BtnLevel"), ("Aux", "BtnAux"), ("Control", "BtnControl"),
                                     ("Auto", "BtnAuto")), (826, 882, 937, 1018, 1074)):
        button(label, template, px(85, y, 92, 36))
    for (l1, t1, y1), (l2, t2, y2) in ((("Perform", "BtnPerform", 825), ("Notes", "BtnVariation", 880)),
                                       (("Lock", "BtnLock", 936), ("Tune", "BtnTune", 1017)),
                                       (("Swing", "BtnSwing", 1072), ("Tempo", "BtnTempo", None)),
                                       (("Grid", "BtnGrid", None), ("Solo", "BtnSolo", None)),
                                       (("Mute", "BtnMute", None), ("Select", "BtnSelect", None))):
        for label, template, y, column in ((l1, t1, y1, None), (l2, t2, y2, None)):
            if y is not None:
                button(label, template, px(1318, y, 92, 36))
            elif label == "Select":
                button(label, template, px(1320, 1169, 92, 50))
            else:
                index = {"Tempo": 4, "Grid": 5, "Solo": 6, "Mute": 7}[label]
                button(label, template, px(columns[index], 1170, 94, 52))
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
