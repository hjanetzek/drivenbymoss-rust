#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""Feed a running APC40 mkII, Launchpad Pro Mk3 or Launch Control XL simulator as DrivenByMoss would: the identity
request (prints the reply), the startup SysEx, pad and button LEDs, and the APC's LED rings or the
Launchpad's SysEx LED update with RGB and the logo, or the Launch Control XL's template
selection, two-colour LEDs, flashing and SysEx LED update.

Usage: midi_feed.py [--model apc40mk2|lppromk3|lcxl] [--suffix " (sim)"]
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from midi_simulator import DEVICES  # noqa: E402
from sim_common.alsa_seq import AlsaSeq  # noqa: E402

IDENTITY_REQUEST = bytes([0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7])


def messages(model: str) -> list[bytes]:
    out = [IDENTITY_REQUEST]
    if model == "apc40mk2":
        out.append(bytes.fromhex("F0 47 7F 29 60 00 04 41 08 02 01 F7"))
        for n in range(40):
            out.append(bytes([0x90, n, 5 + (n % 12) * 4]))  # pads: palette hues
        out.append(bytes([0x9C, 39, 21]))  # top right pad blinking (channel 13)
        for i in range(5):
            out.append(bytes([0x90, 0x52 + i, 21]))  # scene buttons green
        for i in range(8):
            out.append(bytes([0x90 | i, 0x33, 1 if i == 0 else 0]))  # track select
            out.append(bytes([0x90 | i, 0x32, 1]))  # activator
            out.append(bytes([0x90 | i, 0x30, 1 if i % 2 else 0]))  # arm
            out.append(bytes([0x90 | i, 0x42, i % 3]))  # A/B
            out.append(bytes([0xB0, 0x38 + i, (1, 2, 3)[i % 3]]))  # track knob ring modes
            out.append(bytes([0xB0, 0x30 + i, 16 * i + 8]))  # and values
            out.append(bytes([0xB0, 0x18 + i, 2]))  # device rings: volume
            out.append(bytes([0xB0, 0x10 + i, 127 - 15 * i]))
        out.append(bytes([0x90, 0x5B, 1]))  # Play
        out.append(bytes([0x90, 0x57, 1]))  # Pan
    elif model == "lcxl":
        header = bytes.fromhex("F0 00 20 29 02 11")
        out.append(header + bytes([0x77, 0x08, 0xF7]))  # factory template 1 (the device answers)
        for column in range(8):
            for row in range(3):
                red, green = (column + row) % 4, (column // 2 + row) % 4
                out.append(bytes([0x98, 13 + row + 16 * column, 12 + red + 16 * green]))
            out.append(bytes([0x98, (41, 42, 43, 44, 57, 58, 59, 60)[column], 0x3C if column == 0 else 0x1D]))
            out.append(bytes([0x98, (73, 74, 75, 76, 89, 90, 91, 92)[column], 0x0F]))
        out.append(bytes([0x98, 106, 0x3F]))  # Mute: amber
        out.append(bytes([0xB8, 104, 0x3C]))  # Send up: green
        out.append(bytes([0xB8, 0x00, 0x28]))  # flashing on
        out.append(bytes([0x98, 108, 0x0B]))  # Record Arm: red, flashing (flags 8)
        out.append(header + bytes([0x78, 0x08, 44 + 3, 0x3F, 0xF7]))  # SysEx LED update: ▶ amber
    else:
        header = bytes.fromhex("F0 00 20 29 02 0E")
        out.append(header + bytes([0x10, 0x00, 0xF7]))
        out.append(header + bytes([0x0E, 0x01, 0xF7]))
        for row in range(8):
            for col in range(8):
                out.append(bytes([0x90, (row + 1) * 10 + col + 1, 5 + ((row + col) % 12) * 4]))
        for i in range(8):
            out.append(bytes([0xB0, 89 - i * 10, 21]))  # scenes green
            out.append(bytes([0xB0, 101 + i, 45]))  # track select blue
        out.append(bytes([0xB0, 20, 21]))  # Play
        out.append(bytes([0xB0, 10, 5]))  # Record
        out.append(bytes([0x91, 88, 5]))  # top right pad flashing
        # SysEx LED update: an RGB pad (pure magenta), a pulsing pad, the logo white
        out.append(header + bytes([0x03, 0x03, 11, 0x7F, 0x00, 0x7F, 0x02, 12, 45, 0x00, 99, 3, 0xF7]))
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="apc40mk2", choices=tuple(DEVICES))
    parser.add_argument("--suffix", default=" (sim)")
    args = parser.parse_args()
    device = DEVICES[args.model]
    name = device.PORTS[device.SEND_PORT] + args.suffix
    seq = AlsaSeq("dbm-midi-feed", ["io"])
    target = seq.find_port(name)
    if target is None:
        sys.exit(f"simulator port {name!r} not found")
    seq.connect_to(0, *target)
    seq.connect_from(0, *target)
    replies = []
    seq.start_reader(lambda data, port: replies.append(data))
    for message in messages(args.model):
        seq.send(message, 0)
    time.sleep(0.3)
    identity = [r for r in replies if r[:2] == b"\xF0\x7E"]
    print("identity reply:", identity[0].hex(" ") if identity else "none")
    seq.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
