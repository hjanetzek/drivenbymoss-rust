# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""What NI's driver (and dbm-nihost) adds to the template MIDI of the Maschines: the colour
palette, NI's JAM SysEx and the DrivenByMoss messages between the core and dbm-nihost
(support/linux/ni_host: README "DrivenByMoss messages", src/midi.h, src/palette.cpp,
src/jam_host.cpp)."""

# NI's colour palette (hue * 4 + level) as dbm-nihost shows it (palette.cpp): 0..71 from cabl's
# JAM palette (MIT license, (C) 2015 Vincenzo Pacella), 72..79 greys and white
PALETTE = (
    (0, 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, 0), (32, 0, 0),
    (64, 0, 0), (255, 0, 0), (255, 32, 16), (32, 16, 0), (64, 16, 0),
    (255, 32, 0), (255, 64, 32), (32, 48, 8), (64, 64, 0), (255, 96, 0),
    (255, 128, 48), (40, 56, 0), (64, 96, 0), (255, 224, 0), (128, 255, 64),
    (24, 40, 0), (96, 176, 0), (96, 255, 0), (112, 255, 64), (16, 40, 0),
    (56, 112, 0), (72, 255, 0), (64, 255, 64), (0, 40, 0), (0, 96, 0),
    (0, 255, 0), (48, 255, 48), (0, 40, 16), (0, 112, 32), (0, 255, 48),
    (56, 255, 40), (0, 40, 40), (0, 112, 96), (0, 255, 226), (64, 255, 255),
    (0, 24, 48), (0, 64, 112), (0, 96, 255), (40, 160, 255), (0, 0, 40),
    (0, 0, 128), (0, 0, 255), (40, 104, 255), (8, 0, 44), (32, 0, 128),
    (40, 0, 255), (48, 88, 255), (24, 0, 40), (72, 0, 128), (104, 0, 255),
    (88, 80, 255), (32, 0, 48), (96, 0, 96), (255, 0, 128), (255, 64, 160),
    (32, 0, 16), (90, 0, 48), (255, 0, 64), (255, 40, 104), (40, 0, 8),
    (112, 0, 16), (255, 0, 16), (255, 48, 56), (40, 40, 40), (96, 96, 96),
    (226, 226, 226), (160, 255, 255), (40, 40, 40), (96, 96, 96), (226, 226, 226),
    (255, 255, 255), (24, 24, 24), (64, 64, 64), (255, 255, 255), (255, 255, 255),
)


def palette_rgb(index: int) -> tuple[int, int, int]:
    if 0 <= index < len(PALETTE):
        return PALETTE[index]
    return (0, 0, 0)


def colour(hue: int, level: int) -> int:
    """NI's colour index of a hue (1..17) and level (0 dim, 1 medium, 2 full, 3 pale)."""
    return 0 if hue == 0 else hue * 4 + (level & 3)


HUE_WHITE = 17
LEVEL_FULL = 2

# The DrivenByMoss messages: F0 7D 44 42 4D <id> ...
DBM_HEADER = bytes([0xF0, 0x7D, 0x44, 0x42, 0x4D])
DBM_LABELS = 0x10  # F0 7D 44 42 4D 10 <position> <characters> F7: the Mk2's button labels
DBM_HOST = 0x11  # F0 7D 44 42 4D 11 F7: the query; 11 01 F7: the Mk2's hello (Shift is sent)
DBM_SHIFT = 0x12  # F0 7D 44 42 4D 12 <01 down, 00 up> F7: the Mk2's Shift
DBM_GONE = 0x13  # F0 7D 44 42 4D 13 F7: the device is gone

QUERY = DBM_HEADER + bytes([DBM_HOST, 0xF7])
MK2_HELLO = DBM_HEADER + bytes([DBM_HOST, 0x01, 0xF7])
GONE = DBM_HEADER + bytes([DBM_GONE, 0xF7])


def mk2_shift(down: bool) -> bytes:
    return DBM_HEADER + bytes([DBM_SHIFT, 0x01 if down else 0x00, 0xF7])


# NI's SysEx with the JAM's id (17cc:1500)
JAM_HEADER = bytes([0xF0, 0x00, 0x21, 0x09, 0x15, 0x00, 0x4D, 0x50, 0x00, 0x01])
JAM_SHIFT = 0x4D
JAM_RETURN_FROM_HOST = 0x46
JAM_STRIP_SETUP = 0x05
JAM_STRIP_DUAL_VALUES = 0x04
JAM_STRIP_VALUE_OFFSET = 11
JAM_HELLO = JAM_HEADER + bytes([JAM_RETURN_FROM_HOST, 0x01, 0xF7])


def jam_shift(down: bool) -> bytes:
    return JAM_HEADER + bytes([JAM_SHIFT, 0x01 if down else 0x00, 0xF7])


# Mackie MCU display text: F0 00 00 66 <device> 12 <position> <characters> F7
MCU_HEADER = bytes([0xF0, 0x00, 0x00, 0x66])
MCU_DISPLAY = 0x12


def twos_complement(steps: int) -> int:
    """A relative value as DrivenByMoss decodes it (1 = +1, 127 = -1), at most +-63."""
    steps = max(-63, min(63, int(steps)))
    return steps + 128 if steps < 0 else steps
