# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The controls of Push 1, 2 and 3: MIDI numbers, labels and positions on the drawn surface.

The MIDI numbers are those of DrivenByMoss (``PushControlSurface.PUSH_*`` and
``PushControllerSetup.registerTriggerCommands`` / ``registerContinuousCommands``): buttons are
CCs on channel 1 (127 pressed, 0 released), the encoders relative CCs with a touch note, the pads
the notes 36..99 (36 bottom left), the touch strip pitch bend with the touch note 12.

Positions are in surface units (about millimetres, origin top left); the QML view scales them.
The Push 1 and 2 are an own arrangement after the hardware; the Push 3 is placed from Ableton's
labelled top view diagram (``_push3``).
"""

from dataclasses import dataclass, field

MODELS = ("push1", "push2", "push3")

PAD_FIRST_NOTE = 36
PAD_COUNT = 64
STRIP_TOUCH_NOTE = 12

# Grid of the drawing
PAD_X0 = 36.0
PAD_Y0 = 64.0
PAD_PITCH = 16.0
PAD_SIZE = 14.0
SURFACE_WIDTH = 218.0
SURFACE_HEIGHT = 194.0
# The Push 3 is placed from Ableton's labelled top view diagram (see _push3)
PUSH3_SCALE = 0.237
PUSH3_SURFACE = (1594 * PUSH3_SCALE, 1325 * PUSH3_SCALE)


def surface(model: str) -> tuple[float, float]:
    """The size of a model's surface in surface units (about millimetres)."""
    return PUSH3_SURFACE if model == "push3" else (SURFACE_WIDTH, SURFACE_HEIGHT)


@dataclass
class Control:
    kind: str  # "pad", "button", "encoder", "strip", "display"
    label: str
    x: float
    y: float
    w: float
    h: float
    cc: int = -1
    """The CC of a button or encoder."""
    note: int = -1
    """The note of a pad, the touch note of an encoder or of the touch strip."""
    rgb: bool = False
    """True for a button with an RGB LED (colour from the palette), else a white/single LED."""
    base: tuple = field(default=(255, 255, 255))
    """Push 1: the colour of a single colour button LED."""
    help_label: str = ""
    """The name of the control in its help box (empty: the label, or the core's label)."""
    help_side: str = ""
    """The side of the panel for the control's help box (empty: the nearest, sim_common.help)."""

    def as_dict(self) -> dict:
        return {"kind": self.kind, "label": self.label, "x": self.x, "y": self.y, "w": self.w, "h": self.h,
                "cc": self.cc, "note": self.note, "rgb": self.rgb}


def pad_note(column: int, row_from_top: int) -> int:
    return PAD_FIRST_NOTE + (7 - row_from_top) * 8 + column


def _col_x(column: int) -> float:
    return PAD_X0 + column * PAD_PITCH


def build_controls(model: str) -> list[Control]:
    """All controls of a model."""
    if model not in MODELS:
        raise ValueError(f"unknown model {model!r}")
    if model == "push3":
        return _push3()
    push1 = model == "push1"
    push3 = model == "push3"
    c: list[Control] = []

    # The help boxes of the buttons right of the display go above the panel (the right column has
    # enough boxes)
    top_right = {30, 59, 52, 110, 112, 32, 33, 111, 113, 28, 53, 114, 115}

    def button(label, cc, x, y, w=14.0, h=6.0, rgb=False, base=(255, 255, 255), help_label=""):
        side = "top" if cc in top_right and y < 60 else ""
        c.append(Control("button", label, x, y, w, h, cc=cc, rgb=rgb, base=base, help_label=help_label, help_side=side))

    def encoder(label, cc, touch, x, y, size=12.0, help_label="", help_side=""):
        c.append(Control("encoder", label, x, y, size, size, cc=cc, note=touch, help_label=help_label, help_side=help_side))

    # The 8 encoders over the display, the small encoders on the left, master on the right
    for i in range(8):
        encoder(f"{i + 1}", 71 + i, i, _col_x(i) + 1, 2, help_label=f"Knob {i + 1}")
    encoder("Tempo", 14, 10, 4, 2)
    if push3:
        # Push 3: the tempo/swing encoder can be pressed (CC 15), no second small encoder
        button("Swing", 15, 20, 5, 12, 5)
    else:
        encoder("Swing", 15, 9, 20, 2)
    encoder("Master", 79, 8, 200, 2, help_side="top")
    if push3:
        c.append(Control("button", "Cue", 186, 5, 12, 5, cc=111, help_side="top"))
    button("Pedal 1", 64, 168, 3, 13, 4)
    button("Pedal 2", 69, 168, 9, 13, 4)

    # Display and the two button rows (Push 2/3: row 2 above, row 1 below the display; Push 1:
    # both below its text display)
    if push1:
        c.append(Control("display", "Display", PAD_X0, 18, 8 * PAD_PITCH - 2, 26))
        row1_y, row2_y = 46.0, 54.0
    else:
        c.append(Control("display", "Display", PAD_X0, 27, 8 * PAD_PITCH - 2, 22))
        row1_y, row2_y = 52.0, 18.0
    for i in range(8):
        button("", 20 + i, _col_x(i), row1_y, rgb=True, help_label=f"{'Row 1' if push1 else 'Below the display'} {i + 1}")
        button("", 102 + i, _col_x(i), row2_y, rgb=True, help_label=f"{'Row 2' if push1 else 'Above the display'} {i + 1}")

    # Pads and scene buttons (top scene: CC 43)
    for row in range(8):
        for col in range(8):
            c.append(Control("pad", "", _col_x(col), PAD_Y0 + row * PAD_PITCH, PAD_SIZE, PAD_SIZE, note=pad_note(col, row)))
        button(("1/4", "1/4t", "1/8", "1/8t", "1/16", "1/16t", "1/32", "1/32t")[row], 43 - row, 166,
               PAD_Y0 + row * PAD_PITCH + 2, 12, 10, rgb=True, help_label=f"Scene {row + 1}")

    # Left side
    button("Tap", 3, 4, 18)
    button("Metronome", 9, 20, 18, 12, 6)
    button("Delete", 118, 4, 27, 14, 9)
    button("Undo", 119, 4, 38, 14, 9)
    button("Mute", 60, 4, 52, 9, 6, rgb=not push1, base=(255, 140, 0))
    button("Solo", 61, 14, 52, 9, 6, rgb=not push1, base=(40, 80, 255))
    button("Stop", 29, 24, 52, 9, 6, rgb=not push1, base=(255, 0, 0))
    left = [("Convert", 35), ("Double", 117), ("Quantize", 116), ("Fixed Len", 90), ("Automate", 89),
            ("Duplicate", 88), ("New", 92 if push3 else 87), ("Record", 86), ("Play", 85)]
    if push1:
        left[0] = ("Master", 28)
    for i, (label, cc) in enumerate(left):
        rgb = cc in (89, 86, 85) and not push1
        base = {85: (0, 230, 0), 86: (255, 0, 0), 89: (255, 0, 0)}.get(cc, (255, 180, 0))
        button(label, cc, 4, PAD_Y0 + i * 14, 14, 12, rgb=rgb, base=base)
    c.append(Control("strip", "", 22, PAD_Y0, 8, 7 * PAD_PITCH + PAD_SIZE, note=STRIP_TOUCH_NOTE))

    # Right side
    if push1:
        button("Volume", 114, 184, 18)
        button("Pan & Send", 115, 200, 18)
        button("Track", 112, 184, 27, 14, 9)
        button("Clip", 113, 200, 27, 14, 9)
        button("Device", 110, 184, 38, 14, 9)
        button("Browse", 111, 200, 38, 14, 9)
        button("Add Effect", 52, 166, 27, 12, 9)
        button("Add Track", 53, 166, 38, 12, 9)
        button("User", 59, 166, 18, 12, 6)
    else:
        button("Setup", 30, 184, 18)
        button("User", 59, 200, 18)
        button("Add Device", 52, 166, 27, 12, 9)
        button("Device", 110, 184, 27, 14, 9)
        button("Mix", 112, 200, 27, 14, 9)
        if push3:
            button("Add", 32, 166, 38, 12, 9)
            button("Swap", 33, 184, 38, 14, 9)
        else:
            button("Add Track", 53, 166, 38, 12, 9)
            button("Browse", 111, 184, 38, 14, 9)
        button("Clip", 113, 200, 38, 14, 9)
        button("Master", 28, 166, 52, 12, 6, rgb=push3)
    button("Up", 46, 195, 47, 8, 6)
    button("Left", 44, 184, 54, 9, 6)
    button("Right", 45, 205, 54, 9, 6)
    button("Down", 47, 195, 61, 8, 6)
    if push3:
        button("•", 91, 195, 54, 8, 6)
        # The jog wheel: turn (CC 70), press (CC 94), tilt left/right (CC 93/95)
        encoder("Jog", 70, -1, 192, 72, 14, help_label="Jog wheel")
        button("◂", 93, 184, 76, 7, 6, help_label="Jog ◂")
        button("▸", 95, 207, 76, 7, 6, help_label="Jog ▸")
        button("Push", 94, 194, 88, 10, 5, help_label="Jog push")
        button("Capture", 65, 184, 96, 14, 6, rgb=True)
        button("Scene View", 34, 200, 96, 14, 6)
        button("Lock", 83, 184, 104, 14, 6, rgb=True)
        button("Load", 80, 184, 112, 9, 6)
        button("Save", 82, 194, 112, 9, 6)
        button("Help", 81, 204, 112, 10, 6)
        right_y0 = 122.0
        step = 9.0
    else:
        right_y0 = 100.0
        step = 12.0
    button("Repeat", 56, 184, right_y0)
    button("Accent", 57, 200, right_y0)
    button("Scale", 58, 184, right_y0 + step)
    if not push1:
        button("Layout", 31, 200, right_y0 + step)
    button("Note", 50, 184, right_y0 + 2 * step)
    button("Session", 51, 200, right_y0 + 2 * step)
    oct_y = right_y0 + 3 * step + 2
    button("Oct Up", 55, 192, oct_y, 14, 6)
    button("Page ◂", 62, 184, oct_y + 8, 12, 6)
    button("Page ▸", 63, 202, oct_y + 8, 12, 6)
    button("Oct Down", 54, 192, oct_y + 16, 14, 6)
    button("Shift", 49, 184, 180, 14, 9)
    button("Select", 48, 200, 180, 14, 9)
    return c


def _push3() -> list[Control]:
    """The Push 3 as in Ableton's labelled top view diagram (the panel 1594 × 1325 px of a 2061 px
    wide image, 0.237 mm per pixel, the Push 3 being about 378 mm wide): `px(cx, cy, w, h)` places a
    control by its centre and size in the diagram's pixels. The CCs are those of the drawing before
    (the hardware's controls with an icon get short names). Not on the panel, kept for their CCs:
    the pedal inputs (on the back) and Add Device (CC 52, a Push 2 button), small in the bottom
    margin; the jog wheel's nudge left / right and its press, and the press of the two left knobs,
    are small buttons next to the knobs (the hardware moves / presses the knob itself)."""
    c: list[Control] = []
    scale, x0, y0 = PUSH3_SCALE, 229, 54

    def px(cx, cy, w, h):
        return ((cx - x0 - w / 2) * scale, (cy - y0 - h / 2) * scale, w * scale, h * scale)

    def button(label, cc, rect, rgb=False, help_label="", help_side=""):
        c.append(Control("button", label, *rect, cc=cc, rgb=rgb, help_label=help_label, help_side=help_side))

    def encoder(label, cc, touch, rect, help_label="", help_side=""):
        c.append(Control("encoder", label, *rect, cc=cc, note=touch, help_label=help_label, help_side=help_side))

    columns = (595, 716.5, 838, 959.5, 1081, 1203, 1324.5, 1446)
    rows = (647.5, 738.5, 830, 921.5, 1013, 1104, 1195, 1287)
    cells = lambda left: [left + 29.4 + i * 58.75 for i in range(4)]  # noqa: E731 - the 4 cells of a block
    # The encoders, the display and its buttons
    for i, x in enumerate(columns):
        encoder(f"{i + 1}", 71 + i, i, px(x, 144, 50, 50), help_label=f"Knob {i + 1}")
    c.append(Control("display", "Display", *px(1020.5, 390.5, 969, 173)))
    for i, x in enumerate(columns):
        button("", 20 + i, px(x, 548.5, 107, 51), rgb=True, help_label=f"Below the display {i + 1}", help_side="left")
        button("", 102 + i, px(x, 232.5, 107, 51), rgb=True, help_label=f"Above the display {i + 1}", help_side="top")
    # Upper left: Sets, Setup, Learn, User; upper right: Device, Mix, Clip, Session (show the clips)
    for x, (label, cc) in zip(cells(261), (("Sets", 80), ("Setup", 30), ("Learn", 81), ("User", 59))):
        button(label, cc, px(x, 232.5, 55, 51), help_side="top")
    for x, (label, cc) in zip(cells(1545), (("Device", 110), ("Mix", 112), ("Clip", 113), ("Session", 34))):
        button(label, cc, px(x, 232, 55, 51), help_label="Session (clips)" if cc == 34 else "", help_side="top")
    # The display row: the volume knob (its press under it), Undo / Save; Add / Swap, the jog wheel
    encoder("Volume", 79, 8, px(317.5, 390, 99, 100), help_label="Volume knob")
    button("Cue", 111, px(317.5, 468, 64, 24), help_label="Volume knob press")
    button("Undo", 119, px(470, 329.5, 52, 51))
    button("Save", 82, px(470, 451.5, 52, 51))
    button("+", 32, px(1570.5, 328.5, 51, 51), help_label="Add")
    button("Swap", 33, px(1570.5, 451.5, 51, 51))
    encoder("Jog", 70, -1, px(1707, 390, 158, 158), help_label="Jog wheel")
    button("◂", 93, px(1650, 496, 40, 26), help_label="Jog ◂")
    button("Push", 94, px(1707, 496, 50, 26), help_label="Jog push")
    button("▸", 95, px(1764, 496, 40, 26), help_label="Jog ▸")
    # Below the display: Lock, Stop Clip, Mute, Solo; Master Track; the session d-pad
    for x, (label, cc, rgb) in zip(cells(261), (("Lock", 83, True), ("Stop", 29, True), ("M", 60, True), ("S", 61, True))):
        button(label, cc, px(x, 548.5, 55, 51), rgb=rgb, help_label={29: "Stop Clip", 60: "Mute", 61: "Solo"}.get(cc, ""))
    button("Master", 28, px(1570.5, 548.5, 51, 51), rgb=True, help_label="Master Track")
    button("▴", 46, px(1707, 545, 44, 32), help_label="Up")
    button("◂", 44, px(1650, 605, 40, 40), help_label="Left")
    button("•", 91, px(1707, 605, 44, 44), help_label="Center")
    button("▸", 45, px(1764, 605, 40, 40), help_label="Right")
    button("▾", 47, px(1707, 665, 44, 32), help_label="Down")
    # The left column: the swing & tempo knob (its press right of it), Tap Tempo / Metronome /
    # Quantize, Fixed Length / Automate, New / Capture / Record, Play; the touch strip
    encoder("Tempo", 14, 10, px(317, 641, 66, 66), help_label="Swing & tempo knob")
    button("Swing", 15, px(380, 641, 44, 26), help_label="Swing & tempo press")
    for label, cc, y, h, rgb in (("Tap Tempo", 3, 727, 54, False), ("Metronome", 9, 784, 54, False), ("Quantize", 116, 841, 54, False),
                                 ("Fixed Length", 90, 944.5, 44, False), ("Automate", 89, 990.5, 44, True),
                                 ("New", 92, 1093, 54, False), ("Capture", 65, 1150, 54, True), ("Record", 86, 1207, 54, True),
                                 ("Play", 85, 1287, 78, True)):
        button(label, cc, px(317.5, y, 113, h), rgb=rgb)
    c.append(Control("strip", "", *px(453, 966, 84, 722), note=STRIP_TOUCH_NOTE))
    # The pads and the scene buttons (top: 1/32t, CC 43)
    for row, y in enumerate(rows):
        for col, x in enumerate(columns):
            c.append(Control("pad", "", *px(x, y, 110, 80), note=pad_note(col, row)))
        button(("1/32t", "1/32", "1/16t", "1/16", "1/8t", "1/8", "1/4t", "1/4")[row], 43 - row, px(1570.5, y, 51, 78), rgb=True,
               help_label=f"Scene {row + 1}")
    # The right column: Note / Session, Scale / Layout; Repeat / Accent; Double Loop / Duplicate,
    # Convert / Delete; Octave up, Page ◂ ▸, Octave down; Shift / Select
    left, right = 1666, 1748
    for label, cc, x, y, h in (("Note", 50, left, 727, 54), ("Session", 51, right, 727, 54), ("Scale", 58, left, 784, 44),
                               ("Layout", 31, right, 784, 44), ("Repeat", 56, left, 921, 76), ("Accent", 57, right, 921, 76),
                               ("Double Loop", 117, left, 1004, 60), ("Duplicate", 88, right, 1004, 60), ("Convert", 35, left, 1067, 60),
                               ("Delete", 118, right, 1067, 60), ("Shift", 49, left, 1307, 38), ("Select", 48, right, 1307, 38)):
        button(label, cc, px(x, y, 80, h))
    button("Octave ▴", 55, px(1707, 1140, 84, 34))
    button("Page ◂", 62, px(1652, 1192, 56, 40))
    button("Page ▸", 63, px(1762, 1192, 56, 40))
    button("Octave ▾", 54, px(1707, 1245, 84, 34))
    # Not on the panel (kept for their CCs): the pedals, Add Device
    button("Pedal 1", 64, px(290, 1353, 56, 22))
    button("Pedal 2", 69, px(350, 1353, 56, 22))
    button("Add Device", 52, px(453, 1353, 80, 22))
    return c


def rgb_ccs(model: str) -> frozenset[int]:
    return frozenset(control.cc for control in build_controls(model) if control.kind == "button" and control.rgb)


def button_by_cc(model: str) -> dict[int, Control]:
    return {control.cc: control for control in build_controls(model) if control.kind == "button"}


def encoder_by_cc(model: str) -> dict[int, Control]:
    return {control.cc: control for control in build_controls(model) if control.kind == "encoder"}


PORT_STYLES = ("jack", "numbered")


def midi_names(model: str, style: str = "jack") -> tuple[str, str, str]:
    """The ALSA sequencer client name and the names of the Live and the User port.

    "jack": as current Linux kernels name the ports of the real device from its USB MIDI jack
    names ("Ableton Push 2 Live Port"; also DrivenByMoss' Push 3 discovery name on Linux and the
    macOS names). "numbered": as older kernels do ("Ableton Push 2 MIDI 1", "... MIDI 2").
    """
    client = {"push1": "Ableton Push", "push2": "Ableton Push 2", "push3": "Ableton Push 3"}[model]
    if style == "numbered":
        return client, f"{client} MIDI 1", f"{client} MIDI 2"
    return client, f"{client} Live Port", f"{client} User Port"
