#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# Prints the OSC messages of the "Open Sound Control" controller of the Java DrivenByMoss, to see
# what changes in Bitwig when a controller of the Java version is used (and compare it with the
# Rust version). Configure the OSC controller in Bitwig to send to this host and port.
#
#   osc_monitor.py [--port 9000] [--filter /track/] [--quiet-vu] [--send /path value ...]
#
# Without --send it listens and prints "time address values". With --send it sends one message
# to the OSC controller's receive port (--send-port, default 8000) and exits.

import argparse
import socket
import struct
import sys
import time


def read_string(data, pos):
    end = data.index(b"\0", pos)
    text = data[pos:end].decode("utf-8", "replace")
    pos = end + 1
    return text, (pos + 3) & ~3


def parse_message(data):
    address, pos = read_string(data, 0)
    if address == "#bundle":
        # Bundle: time tag, then size-prefixed elements
        pos = 16
        messages = []
        while pos + 4 <= len(data):
            size = struct.unpack(">i", data[pos:pos + 4])[0]
            messages.extend(parse_message(data[pos + 4:pos + 4 + size]))
            pos += 4 + size
        return messages
    if pos >= len(data):
        return [(address, [])]
    tags, pos = read_string(data, pos)
    values = []
    for tag in tags[1:]:
        if tag == "i":
            values.append(struct.unpack(">i", data[pos:pos + 4])[0])
            pos += 4
        elif tag == "f":
            values.append(round(struct.unpack(">f", data[pos:pos + 4])[0], 4))
            pos += 4
        elif tag == "d":
            values.append(round(struct.unpack(">d", data[pos:pos + 8])[0], 4))
            pos += 8
        elif tag == "s":
            text, pos = read_string(data, pos)
            values.append(text)
        elif tag == "T":
            values.append(True)
        elif tag == "F":
            values.append(False)
        elif tag == "b":
            size = struct.unpack(">i", data[pos:pos + 4])[0]
            values.append(f"<blob {size}>")
            pos += (4 + size + 3) & ~3
    return [(address, values)]


def osc_string(text):
    data = text.encode("utf-8") + b"\0"
    return data + b"\0" * (-len(data) % 4)


def build_message(address, args):
    tags = ","
    payload = b""
    for arg in args:
        try:
            payload += struct.pack(">i", int(arg))
            tags += "i"
        except ValueError:
            try:
                payload += struct.pack(">f", float(arg))
                tags += "f"
            except ValueError:
                payload += osc_string(arg)
                tags += "s"
    return osc_string(address) + osc_string(tags) + payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=9000, help="port to listen on (the OSC controller's send port)")
    parser.add_argument("--filter", action="append", default=[], help="only print addresses starting with this (repeatable)")
    parser.add_argument("--quiet-vu", action="store_true", help="hide VU meter and play position messages")
    parser.add_argument("--send", nargs="+", metavar=("ADDRESS", "VALUE"), help="send one message and exit")
    parser.add_argument("--send-host", default="127.0.0.1")
    parser.add_argument("--send-port", type=int, default=8000, help="the OSC controller's receive port")
    args = parser.parse_args()

    if args.send:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.sendto(build_message(args.send[0], args.send[1:]), (args.send_host, args.send_port))
        return

    noisy = ("/vu", "/time", "/beat", "/position")
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(("0.0.0.0", args.port))
        print(f"Listening on UDP port {args.port}", file=sys.stderr)
        start = time.monotonic()
        while True:
            data, _ = sock.recvfrom(65536)
            for address, values in parse_message(data):
                if args.filter and not any(address.startswith(f) for f in args.filter):
                    continue
                if args.quiet_vu and any(n in address for n in noisy):
                    continue
                print(f"{time.monotonic() - start:8.3f} {address} {' '.join(str(v) for v in values)}", flush=True)


if __name__ == "__main__":
    main()
