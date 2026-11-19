#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""Feed a running simulator as a host would: a test frame over the display socket and LED
messages over ALSA (pads in palette colours, some buttons, a blinking and a pulsing pad, the touch
strip as a bar). Also a quick check that the simulator answers the identity request.

Usage: demo_feed.py [--model push2] [--display-socket PATH] [--frames N] [--fps F] [--no-midi]
"""

import argparse
import os
import socket
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from push_simulator import display, layout  # noqa: E402


def test_frame(phase: int = 0) -> bytes:
    """Colour bars on top, a grey ramp, a white frame border and 8 "cells" like the DrivenByMoss
    track mode; built line by line (fast enough for a few fps)."""
    bars = [(255, 255, 255), (255, 255, 0), (0, 255, 255), (0, 255, 0), (255, 0, 255), (255, 0, 0), (0, 0, 255), (0, 0, 0)]
    lines = []
    for y in range(display.HEIGHT):
        row = bytearray(display.LINE_BYTES)
        for x in range(display.WIDTH):
            if y < 4 or y >= display.HEIGHT - 4 or x < 4 or x >= display.WIDTH - 4:
                rgb = (255, 255, 255)
            elif y < 60:
                rgb = bars[((x + phase * 8) // 120) % 8]
            elif y < 80:
                v = x * 255 // display.WIDTH
                rgb = (v, v, v)
            else:
                cell = x // 120
                inner = 8 < x % 120 < 112 and 90 < y < 150
                fill = 90 + (cell * 7 + phase) % 50
                rgb = (40, 120, 200) if inner and y > 150 - fill * 60 // 140 else ((50, 50, 55) if inner else (20, 20, 22))
            p = display.rgb_to_pixel(*rgb)
            row[2 * x] = p & 0xFF
            row[2 * x + 1] = p >> 8
        lines.append(bytes(row))
    return display.HEADER + display.xor_mask(b"".join(lines))


def send_frames(path: str, count: int, fps: float) -> None:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(path)
    # Some garbage first: the simulator has to resynchronise on the header
    s.sendall(b"\x01\x02garbage\xff\xcc")
    for i in range(count):
        data = test_frame(i)
        # Two writes as the USB transfer (header block, image block); partial writes are fine
        s.sendall(data[:16])
        s.sendall(data[16:100000])
        s.sendall(data[100000:])
        time.sleep(1.0 / fps)
    s.close()


def send_midi(model: str) -> None:
    from push_simulator.alsa_seq import AlsaSeq

    _, live, user = layout.midi_names(model)
    target = user if model == "push1" else live
    seq = AlsaSeq("dbm-push-demo-feed", ["out"])
    found = seq.find_port(target) or seq.find_port(layout.midi_names(model, "numbered")[2 if model == "push1" else 1])
    if found is None:
        sys.exit(f"simulator port {target!r} not found")
    seq.connect_to(0, *found)
    seq.connect_from(0, *found)
    replies = []
    seq.start_reader(lambda data, port: replies.append(data))
    seq.send(bytes([0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]), 0)
    for i in range(64):
        seq.send(bytes([0x90, 36 + i, (i * 2 + 5) % 128]), 0)
    seq.send(bytes([0x90, 99, 5]), 0)
    seq.send(bytes([0x9E, 99, 21]), 0)  # blinking (channel 15) between red and green
    seq.send(bytes([0x90, 92, 0]), 0)
    seq.send(bytes([0x9A, 92, 45]), 0)  # pulsing (channel 11) to blue
    for cc, value in ((85, 21), (86, 5), (60, 9), (61, 13), (29, 6), (20, 5), (21, 9), (22, 13), (23, 21),
                      (102, 45), (103, 49), (43, 21), (42, 9), (49, 127), (50, 30), (51, 127), (118, 30)):
        seq.send(bytes([0xB0, cc, value]), 0)
    seq.send(bytes([0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01, 0x17, 0x01, 0xF7]), 0)  # strip: host LEDs, bar
    seq.send(bytes([0xE0, 0x00, 0x50]), 0)  # strip value
    if model == "push1":
        for row, text in enumerate(("Push 1 simulator", "4 x 68 characters", "", "Track 1  Track 2")):
            seq.send(bytes([0xF0, 0x47, 0x7F, 0x15, 0x18 + row, 0x00, 0x45, 0x00]) + text.ljust(68).encode() + b"\xF7", 0)
    time.sleep(0.3)
    identity = [r for r in replies if r[:2] == b"\xF0\x7E"]
    print("identity reply:", identity[0].hex(" ") if identity else "none")
    seq.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="push2", choices=layout.MODELS)
    parser.add_argument("--display-socket", default=None)
    parser.add_argument("--frames", type=int, default=3)
    parser.add_argument("--fps", type=float, default=10.0)
    parser.add_argument("--no-midi", action="store_true")
    args = parser.parse_args()
    if not args.no_midi:
        send_midi(args.model)
    if args.model != "push1" and args.frames > 0:
        send_frames(args.display_socket or display.default_socket_path(), args.frames, args.fps)
        print(f"sent {args.frames} frames")
    return 0


if __name__ == "__main__":
    sys.exit(main())
