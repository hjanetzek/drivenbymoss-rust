#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""Feed a running Maschine simulator as DrivenByMoss would: the query (prints the hello), pad,
group and button LEDs, and the Mk2's display text and button labels or the JAM's strip setup,
strip values and level meters.

Usage: maschine_feed.py [--model mk2|jam] [--suffix " (sim)"]
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from maschine_simulator import layout, ni  # noqa: E402
from sim_common.alsa_seq import AlsaSeq  # noqa: E402


def messages(model: str) -> list[bytes]:
    out = [ni.QUERY]
    if model in ("mk3", "mikromk3"):
        for i in range(16):
            out.append(bytes([0x90, 36 + i, 4 + 4 * i + 2]))  # pads: NI's hues, full level
        if model == "mk3":
            for i in range(8):
                out.append(bytes([0xB0, 100 + i, 6 + 8 * i]))  # groups
            out.append(bytes([0xB0, 30, 127]))  # the encoder's up LED
            line1 = "".join(f"{t:7s}" for t in ("Synth", "Drums", "Bass", "Vocals", "Audio5", "FX 1", "FX 2", "Master"))
            out.append(ni.MCU_HEADER + bytes([0x14, ni.MCU_DISPLAY, 0]) + line1.encode() + b"\xF7")
        for cc in (57, 58, 44):  # Play, Rec, Volume
            out.append(bytes([0xB0, cc, 127]))
        out.append(bytes([0xB0, 1, 90]))  # the touch strip LEDs
    elif model == "mk2":
        for i in range(16):
            out.append(bytes([0x90, 36 + i, 4 + 4 * i]))  # pads: NI's hues
        for i in range(8):
            out.append(bytes([0xB0, 100 + i, 6 + 8 * i]))  # groups
        for cc in (57, 58, 83, 41):  # Play, Rec, Step, Control
            out.append(bytes([0xB0, cc, 127]))
        out.append(bytes([0x90, 28, 127]))  # the 7th display button (a note)
        labels = "".join(f">{t:6s}" if i == 0 else f" {t:6s}" for i, t in enumerate(("Volume", "Pan", "Send-", "Send+", "PinTrk", "TrkOn", "User", "Params")))
        out.append(ni.DBM_HEADER + bytes([ni.DBM_LABELS, 0]) + labels.encode() + b"\xF7")
        line1 = "".join(f"{t:7s}" for t in ("Synth", "Drums", "Bass", "Vocals", "Audio5", "FX 1", "FX 2", "Master"))
        line2 = "".join(f"{t:7s}" for t in ("-3.2dB", "0.0dB", "-12dB", "-inf", "0.0dB", "-6.0dB", "0.0dB", "0.0dB"))
        out.append(ni.MCU_HEADER + bytes([0x14, ni.MCU_DISPLAY, 0]) + line1.encode() + b"\xF7")
        out.append(ni.MCU_HEADER + bytes([0x14, ni.MCU_DISPLAY, 56]) + line2.encode() + b"\xF7")
    else:
        for row in range(8):
            for col in range(8):
                out.append(bytes([0x90, 36 + (7 - row) * 8 + col, 4 + ((row + col) % 16) * 4 + 2]))
        for i in range(8):
            out.append(bytes([0xB0, 80 + i, 30 + i * 4]))  # scenes
            out.append(bytes([0xB0, 70 + i, 6 + i * 8]))  # groups
        setup = bytearray()
        for i in range(8):
            setup += bytes([i % 4, 4 + i * 8 + 2])  # single, dot, pan, dual
        out.append(ni.JAM_HEADER + bytes([ni.JAM_STRIP_SETUP]) + bytes(setup) + b"\xF7")
        out.append(ni.JAM_HEADER + bytes([ni.JAM_STRIP_DUAL_VALUES]) + bytes([100] * 8) + b"\xF7")
        for i in range(8):
            out.append(bytes([0xB0, 8 + i, 20 + i * 15]))  # strip values
        out.append(bytes([0xB0, 64, 90]))  # level meters
        out.append(bytes([0xB0, 65, 120]))
        for cc in (90, 91, 32, 62):  # Play, Rec, Pad Mode, Master
            out.append(bytes([0xB0, cc, 127]))
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="mk2", choices=layout.MODELS)
    parser.add_argument("--suffix", default=" (sim)")
    args = parser.parse_args()
    _, daw_in, daw_out = layout.midi_names(args.model, args.suffix)
    seq = AlsaSeq("dbm-maschine-feed", ["io"])
    source, sink = seq.find_port(daw_in), seq.find_port(daw_out)
    if source is None or sink is None:
        sys.exit(f"simulator ports {daw_in!r}/{daw_out!r} not found")
    seq.connect_from(0, *source)
    seq.connect_to(0, *sink)
    replies = []
    seq.start_reader(lambda data, port: replies.append(data))
    for message in messages(args.model):
        seq.send(message, 0)
    time.sleep(0.3)
    if args.model in ("mk3", "mikromk3"):
        # NI's driver ignores the query of dbm-nihost; it sent its "return from host" on attach
        print("hello: n/a (NI's driver)", " ".join(r.hex(" ") for r in replies if r[0] == 0xF0))
    else:
        hello = ni.MK2_HELLO if args.model == "mk2" else ni.JAM_HELLO
        print("hello:", "yes" if hello in replies else "no", " ".join(r.hex(" ") for r in replies if r[0] == 0xF0))
    seq.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
