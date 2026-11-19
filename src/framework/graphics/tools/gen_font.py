#!/usr/bin/env python3
# Written by Jürgen Moßgraber - mossgrabers.de
# (c) 2017-2026
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Generate src/framework/graphics/font_data.rs: the glyph outlines (quadratic TrueType contours)
of DejaVu Sans for ASCII and Latin-1, which the graphics context rasterizes with anti-aliasing.

Usage (from the repository root):

    python3 src/framework/graphics/tools/gen_font.py [/usr/share/fonts/TTF/DejaVuSans.ttf] \
        > src/framework/graphics/font_data.rs

Only the Python standard library is needed (a minimal TrueType parser: cmap format 4, hmtx,
loca, glyf with composite glyphs). No hinting, no kerning.
"""

import struct
import sys

FONT = sys.argv[1] if len(sys.argv) > 1 else '/usr/share/fonts/TTF/DejaVuSans.ttf'
CODES = list(range(32, 127)) + list(range(160, 256)) + [0x266F, 0x2013, 0x2192, 0x2019]

data = open(FONT, 'rb').read()


def u16(o):
    return struct.unpack_from('>H', data, o)[0]


def i16(o):
    return struct.unpack_from('>h', data, o)[0]


def u32(o):
    return struct.unpack_from('>I', data, o)[0]


tables = {}
for i in range(u16(4)):
    rec = 12 + 16 * i
    tables[data[rec:rec + 4].decode('latin-1')] = u32(rec + 8)

head = tables['head']
units_per_em = u16(head + 18)
long_loca = i16(head + 50) == 1
num_glyphs = u16(tables['maxp'] + 4)
num_hmetrics = u16(tables['hhea'] + 34)
ascender = i16(tables['hhea'] + 4)
descender = i16(tables['hhea'] + 6)


def advance(gid):
    return u16(tables['hmtx'] + 4 * min(gid, num_hmetrics - 1))


def loca(gid):
    if long_loca:
        return u32(tables['loca'] + 4 * gid)
    return 2 * u16(tables['loca'] + 2 * gid)


def cmap():
    base = tables['cmap']
    sub = None
    for i in range(u16(base + 2)):
        rec = base + 4 + 8 * i
        if u16(rec) == 3 and u16(rec + 2) == 1:
            sub = base + u32(rec + 4)
    assert sub is not None and u16(sub) == 4
    seg_count = u16(sub + 6) // 2
    ends = sub + 14
    starts = ends + 2 * seg_count + 2
    deltas = starts + 2 * seg_count
    range_offsets = deltas + 2 * seg_count
    mapping = {}
    for s in range(seg_count):
        start, end = u16(starts + 2 * s), u16(ends + 2 * s)
        delta, ro = u16(deltas + 2 * s), u16(range_offsets + 2 * s)
        for c in range(start, end + 1):
            if c == 0xFFFF:
                continue
            if ro == 0:
                gid = (c + delta) & 0xFFFF
            else:
                gid = u16(range_offsets + 2 * s + ro + 2 * (c - start))
                if gid != 0:
                    gid = (gid + delta) & 0xFFFF
            mapping[c] = gid
    return mapping


def glyph_contours(gid):
    """A list of contours, each a list of (x, y, on_curve)."""
    start, end = loca(gid), loca(gid + 1)
    if start == end:
        return []
    o = tables['glyf'] + start
    n = i16(o)
    if n >= 0:
        ends = [u16(o + 10 + 2 * i) for i in range(n)]
        p = o + 10 + 2 * n
        p += 2 + u16(p)
        count = ends[-1] + 1 if ends else 0
        flags = []
        while len(flags) < count:
            f = data[p]
            p += 1
            flags.append(f)
            if f & 8:
                r = data[p]
                p += 1
                flags.extend([f] * r)
        xs, ys = [], []
        v = 0
        for f in flags:
            if f & 2:
                d = data[p]
                p += 1
                v += d if f & 16 else -d
            elif not f & 16:
                v += i16(p)
                p += 2
            xs.append(v)
        v = 0
        for f in flags:
            if f & 4:
                d = data[p]
                p += 1
                v += d if f & 32 else -d
            elif not f & 32:
                v += i16(p)
                p += 2
            ys.append(v)
        contours = []
        s = 0
        for e in ends:
            contours.append([(xs[i], ys[i], bool(flags[i] & 1)) for i in range(s, e + 1)])
            s = e + 1
        return contours
    # Composite glyph
    contours = []
    p = o + 10
    while True:
        flags, sub = u16(p), u16(p + 2)
        p += 4
        if flags & 1:
            a, b = i16(p), i16(p + 2)
            p += 4
        else:
            a, b = struct.unpack_from('>bb', data, p)
            p += 2
        assert flags & 2, 'point matching is not supported'
        xx, xy, yx, yy = 1.0, 0.0, 0.0, 1.0
        if flags & 8:
            xx = yy = i16(p) / 16384.0
            p += 2
        elif flags & 0x40:
            xx, yy = i16(p) / 16384.0, i16(p + 2) / 16384.0
            p += 4
        elif flags & 0x80:
            xx, xy, yx, yy = (i16(p + 2 * k) / 16384.0 for k in range(4))
            p += 8
        for c in glyph_contours(sub):
            contours.append([(round(x * xx + y * yx + a), round(x * xy + y * yy + b), on) for x, y, on in c])
        if not flags & 0x20:
            break
    return contours


mapping = cmap()
glyphs = []
for code in CODES:
    gid = mapping.get(code, 0)
    if gid == 0 and code != 32:
        continue
    contours = glyph_contours(gid)
    xs = [x for c in contours for x, _, _ in c]
    glyphs.append((code, advance(gid), min(xs) if xs else 0, max(xs) if xs else 0, contours))

t_contours = glyph_contours(mapping[ord('T')])
cap_height = max(y for c in t_contours for _, y, _ in c)

out = sys.stdout
out.write('''// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! GENERATED by `src/framework/graphics/tools/gen_font.py` - do not edit.
//!
//! The glyph outlines of DejaVu Sans (ASCII, Latin-1 and a few symbols) as quadratic TrueType
//! contours in font units.
//!
//! DejaVu fonts: Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is a
//! trademark of Bitstream, Inc. DejaVu changes are in the public domain. The Bitstream Vera
//! license permits to use, copy, merge, publish and distribute the font software (see
//! https://dejavu-fonts.github.io/License.html).

/// A glyph: its code point, advance and horizontal ink extent (font units) and its contours
/// (a range of [`CONTOURS`]).
#[derive(Debug, Clone, Copy)]
pub struct GlyphData {
    pub code: u32,
    pub advance: i16,
    pub x_min: i16,
    pub x_max: i16,
    pub first_contour: u16,
    pub num_contours: u16,
}

''')
out.write('pub const UNITS_PER_EM: f64 = %d.0;\n' % units_per_em)
out.write('/// The height of the letter "T" (used to center texts vertically like Cairo).\n')
out.write('pub const CAP_HEIGHT: f64 = %d.0;\n' % cap_height)
out.write('pub const ASCENDER: f64 = %d.0;\n' % ascender)
out.write('pub const DESCENDER: f64 = %d.0;\n\n' % descender)

contour_ranges = []
points = []
glyph_rows = []
for code, adv, x_min, x_max, contours in glyphs:
    first = len(contour_ranges)
    for c in contours:
        contour_ranges.append((len(points), len(points) + len(c)))
        points.extend(c)
    glyph_rows.append('    GlyphData { code: %d, advance: %d, x_min: %d, x_max: %d, first_contour: %d, num_contours: %d },' % (code, adv, x_min, x_max, first, len(contours)))

out.write('/// The glyphs, sorted by code point.\n#[rustfmt::skip]\npub static GLYPHS: [GlyphData; %d] = [\n' % len(glyph_rows))
out.write('\n'.join(glyph_rows))
out.write('\n];\n\n')


def chunked(items, per_line):
    for i in range(0, len(items), per_line):
        yield '    ' + ' '.join(items[i:i + per_line])


out.write('/// The contours: start and end (exclusive) index into [`POINTS`].\n#[rustfmt::skip]\n')
out.write('pub static CONTOURS: [(u16, u16); %d] = [\n' % len(contour_ranges))
out.write('\n'.join(chunked(['(%d, %d),' % r for r in contour_ranges], 10)))
out.write('\n];\n\n')
out.write('/// The contour points: x, y (font units, y up) and 1 if the point is on the curve.\n#[rustfmt::skip]\n')
out.write('pub static POINTS: [(i16, i16, u8); %d] = [\n' % len(points))
out.write('\n'.join(chunked(['(%d,%d,%d),' % (x, y, 1 if on else 0) for x, y, on in points], 10)))
out.write('\n];\n')
