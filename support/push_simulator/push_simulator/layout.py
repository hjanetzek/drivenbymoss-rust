# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The controls of Push 1, 2 and 3: MIDI numbers, labels and positions on the drawn surface.

The MIDI numbers are those of DrivenByMoss (``PushControlSurface.PUSH_*`` and
``PushControllerSetup.registerTriggerCommands`` / ``registerContinuousCommands``): buttons are
CCs on channel 1 (127 pressed, 0 released), the encoders relative CCs with a touch note, the pads
the notes 36..99 (36 bottom left), the touch strip pitch bend with the touch note 12.

Positions are in surface units (about millimetres, origin top left); the QML view scales them.
The drawing is an own arrangement after the hardware, not Ableton's artwork.
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
