# ------------------------------------------------------------------------------------------
# The runtime (src/protocol/python_runtime.py, the same for every table)

_HEAD = struct.Struct("<HBB4HI")
"""The head of a record without the value: id, type, op, idx[4], len."""
_REC_NONE = struct.Struct("<HBB4HIQ")
_REC_I64 = struct.Struct("<HBB4HIq")
_REC_F64 = struct.Struct("<HBB4HId")
_REC_COLOR = struct.Struct("<HBB4HI4B4x")
_REC_PAIR = struct.Struct("<HBB4HIii")
_I64 = struct.Struct("<q")
_F64 = struct.Struct("<d")
_U32 = struct.Struct("<I")
_PAIR = struct.Struct("<ii")
_COLOR = struct.Struct("<4B")
_INT_DOUBLE = struct.Struct("<i4xd")
_NOTE = struct.Struct("<ddiiiI")
_STEP = struct.Struct("<24d")
_PAD = (b"", b"\0" * 7, b"\0" * 6, b"\0" * 5, b"\0" * 4, b"\0" * 3, b"\0" * 2, b"\0")

HEADER = b"DBM1" + struct.pack("<HH", VERSION_MAJOR, 0)


class ProtocolError(ValueError):
    """A buffer which is not of this protocol (header) or a truncated record."""


def key(id_: int, i0: int = 0, i1: int = 0, i2: int = 0, i3: int = 0) -> int:
    """The key of a value for the change tracking (TYPED_PROTOCOL_PLAN §2.4): ID and indices."""
    return id_ | i0 << 16 | i1 << 32 | i2 << 48 | i3 << 64


# Reading the values (b: the buffer, p: the position of the record, n: the payload size, t: the
# record type)

def _get_none(b, p, n, t):
    return None


def _get_bool(b, p, n, t):
    return _I64.unpack_from(b, p + 16)[0] != 0


def _get_i32(b, p, n, t):
    return _I64.unpack_from(b, p + 16)[0]


def _get_f64(b, p, n, t):
    return _F64.unpack_from(b, p + 16)[0]


def _get_text(b, p, n, t):
    return str(b[p + 24:p + 24 + n], "utf-8", "replace")


def _get_text_list(b, p, n, t):
    q = p + 24
    count = _U32.unpack_from(b, q)[0]
    q += 4
    texts = []
    for _ in range(count):
        size = _U32.unpack_from(b, q)[0]
        q += 4
        texts.append(str(b[q:q + size], "utf-8", "replace"))
        q += size
    return texts


def _get_color(b, p, n, t):
    """(r, g, b) 0..255, None: no color."""
    r, g, b_, valid = _COLOR.unpack_from(b, p + 16)
    return (r, g, b_) if valid else None


def _get_notes(b, p, n, t):
    """(start, end, channel, pitch, velocity, selected, muted) per note."""
    return [(s, e, c, k, v, bool(f & 1), bool(f & 2)) for s, e, c, k, v, f in struct.iter_unpack("<ddiiiI", b[p + 24:p + 24 + n])]


def _get_step(b, p, n, t):
    """The 24 values of a step (StepInfo::to_values), None: an empty step."""
    return _STEP.unpack_from(b, p + 24) if n >= 192 else None


def _get_value_op(b, p, n, t):
    """A double (set, inc, immediately), a bool (touch, indicate) or None (reset); see `op`."""
    if t == T_F64:
        return _F64.unpack_from(b, p + 16)[0]
    if t == T_BOOL:
        return _I64.unpack_from(b, p + 16)[0] != 0
    return None


def _get_pair(b, p, n, t):
    return _PAIR.unpack_from(b, p + 16)


def _get_int_double(b, p, n, t):
    return _INT_DOUBLE.unpack_from(b, p + 24)


GETTERS = {
    "none": _get_none, "bool": _get_bool, "i32": _get_i32, "f64": _get_f64, "text": _get_text,
    "text_list": _get_text_list, "color": _get_color, "notes": _get_notes, "step": _get_step,
    "value_op": _get_value_op, "pair": _get_pair, "int_double": _get_int_double,
}
"""The value readers by value kind."""


def _records(buf):
    """(position, head) of every record; raises ProtocolError for a wrong header."""
    if len(buf) < HEADER_SIZE or bytes(buf[:4]) != b"DBM1" or buf[4] | buf[5] << 8 != VERSION_MAJOR:
        raise ProtocolError("not a buffer of protocol major version %d" % VERSION_MAJOR)
    p, end = HEADER_SIZE, len(buf)
    unpack = _HEAD.unpack_from
    while p + RECORD_SIZE <= end:
        head = unpack(buf, p)
        n = head[7]
        if p + RECORD_SIZE + n > end:
            raise ProtocolError("truncated record at %d" % p)
        yield p, head
        p += RECORD_SIZE + ((n + 7) & ~7)


class Record:
    """The view of a record: the head is unpacked, the value is read when it is accessed. The
    typed views (the classes of CMD_VIEWS and MSG_VIEWS) add the indices by name and `value`."""

    __slots__ = ("h", "b", "p")
    KIND = "none"

    def __init__(self, head, buf, pos):
        self.h = head
        self.b = buf
        self.p = pos

    @property
    def id(self) -> int:
        return self.h[0]

    @property
    def type(self) -> int:
        return self.h[1]

    @property
    def op(self) -> int:
        return self.h[2]

    @property
    def idx(self) -> tuple:
        return self.h[3:7]

    @property
    def value(self):
        h = self.h
        info = CMD_INFO.get(h[0]) or MSG_INFO.get(h[0])
        return GETTERS[info.kind if info else "none"](self.b, self.p, h[7], h[1])

    def __repr__(self) -> str:
        info = CMD_INFO.get(self.id) or MSG_INFO.get(self.id)
        return "<%s %s op=%d value=%r>" % (info.name if info else hex(self.id), self.idx, self.op, self.value)


def iter_records(buf, views=None):
    """The views of the records of a buffer (bytes, bytearray or memoryview): the typed view of
    each known ID (`views`: CMD_VIEWS and MSG_VIEWS by default), else a Record."""
    if views is None:
        views = VIEWS
    get = views.get
    for p, head in _records(buf):
        yield get(head[0], Record)(head, buf, p)


def iter_commands(buf):
    """The views of the commands of a buffer (the typed view of each command)."""
    return iter_records(buf, CMD_VIEWS)


def iter_flat(buf):
    """The records of a buffer without view objects: (id, op, i0, i1, i2, i3, value), the value
    read by its type (bool, int, float, None; the others by the kind of the ID, GETTERS). The
    flat dispatch for hot paths."""
    if len(buf) < HEADER_SIZE or bytes(buf[:4]) != b"DBM1" or buf[4] | buf[5] << 8 != VERSION_MAJOR:
        raise ProtocolError("not a buffer of protocol major version %d" % VERSION_MAJOR)
    p, end = HEADER_SIZE, len(buf)
    unpack_i = _REC_I64.unpack_from
    unpack_d = _F64.unpack_from
    kinds = KINDS
    getters = GETTERS
    while p + 24 <= end:
        id_, t, op, i0, i1, i2, i3, n, v = unpack_i(buf, p)
        if t == 3:  # T_F64
            v = unpack_d(buf, p + 16)[0]
        elif t == 1:  # T_BOOL
            v = v != 0
        elif t == 0:  # T_NONE
            v = None
        elif t != 2:  # not T_I32
            if p + 24 + n > end:
                raise ProtocolError("truncated record at %d" % p)
            v = getters[kinds.get(id_, "none")](buf, p, n, t)
        yield id_, op, i0, i1, i2, i3, v
        p += 24 + ((n + 7) & ~7)


class _WriterBase:
    """A buffer of records on a reusable bytearray (clear() keeps the memory). `buffer()` is the
    buffer with the header for dbm_update_model_bin. The put_* methods take the ID and the four
    index slots; the generated methods of Writer take the fields of a message."""

    __slots__ = ("buf", "size")

    def __init__(self, capacity: int = 4096):
        self.buf = bytearray(max(capacity, 64))
        self.buf[:HEADER_SIZE] = HEADER
        self.size = HEADER_SIZE

    def clear(self) -> None:
        self.size = HEADER_SIZE

    def __len__(self) -> int:
        """The number of bytes (with the header)."""
        return self.size

    def empty(self) -> bool:
        return self.size <= HEADER_SIZE

    def buffer(self) -> bytes:
        """A copy of the buffer (with the header)."""
        return bytes(self.buf[:self.size])

    def _reserve(self, n: int) -> int:
        """The position of n more bytes (the buffer grows)."""
        p = self.size
        if p + n > len(self.buf):
            self.buf.extend(bytes(max(n, len(self.buf))))
        self.size = p + n
        return p

    # The inline values (the hot paths: the reserve inlined)

    def put_none(self, id_, i0=0, i1=0, i2=0, i3=0):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _REC_NONE.pack_into(self.buf, p, id_, 0, 0, i0, i1, i2, i3, 0, 0)
        self.size = p + 24

    def put_bool(self, id_, i0, i1, i2, i3, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _REC_I64.pack_into(self.buf, p, id_, 1, 0, i0, i1, i2, i3, 0, 1 if value else 0)
        self.size = p + 24

    def put_i32(self, id_, i0, i1, i2, i3, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _REC_I64.pack_into(self.buf, p, id_, 2, 0, i0, i1, i2, i3, 0, value)
        self.size = p + 24

    def put_f64(self, id_, i0, i1, i2, i3, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _REC_F64.pack_into(self.buf, p, id_, 3, 0, i0, i1, i2, i3, 0, value)
        self.size = p + 24

    def _put_payload(self, id_, t, i0, i1, i2, i3, payload):
        n = len(payload)
        padded = (n + 7) & ~7
        p = self._reserve(24 + padded)
        _REC_NONE.pack_into(self.buf, p, id_, t, 0, i0, i1, i2, i3, n, 0)
        self.buf[p + 24:p + 24 + padded] = payload + _PAD[n & 7]

    def put_text(self, id_, i0, i1, i2, i3, value):
        self._put_payload(id_, T_TEXT, i0, i1, i2, i3, ("" if value is None else str(value)).encode("utf-8"))

    def put_text_list(self, id_, i0, i1, i2, i3, values):
        parts = [_U32.pack(len(values))]
        for text in values:
            data = str(text).encode("utf-8")
            parts.append(_U32.pack(len(data)))
            parts.append(data)
        self._put_payload(id_, T_TEXT_LIST, i0, i1, i2, i3, b"".join(parts))

    def put_color(self, id_, i0, i1, i2, i3, value):
        """value: (r, g, b) 0..255, None: no color."""
        if value is None:
            _REC_COLOR.pack_into(self.buf, self._reserve(24), id_, T_COLOR, 0, i0, i1, i2, i3, 0, 0, 0, 0, 0)
        else:
            r, g, b = value
            c = lambda v: 0 if v < 0 else 255 if v > 255 else int(v)  # noqa: E731
            _REC_COLOR.pack_into(self.buf, self._reserve(24), id_, T_COLOR, 0, i0, i1, i2, i3, 0, c(r), c(g), c(b), 1)

    def put_notes(self, id_, i0, i1, i2, i3, notes):
        """notes: (start, end, channel, pitch, velocity, selected, muted) per note."""
        pack = _NOTE.pack
        payload = b"".join(pack(s, e, c, k, v, (1 if sel else 0) | (2 if mut else 0)) for s, e, c, k, v, sel, mut in notes)
        self._put_payload(id_, T_STRUCT_ARRAY, i0, i1, i2, i3, payload)

    def put_step(self, id_, i0, i1, i2, i3, values):
        """The 24 values of a step, None: an empty step."""
        self._put_payload(id_, T_STRUCT, i0, i1, i2, i3, b"" if values is None else _STEP.pack(*values))

    def put_pair(self, id_, i0, i1, i2, i3, value):
        _REC_PAIR.pack_into(self.buf, self._reserve(24), id_, T_STRUCT, 0, i0, i1, i2, i3, 0, value[0], value[1])

    def put_int_double(self, id_, i0, i1, i2, i3, value):
        self._put_payload(id_, T_STRUCT, i0, i1, i2, i3, _INT_DOUBLE.pack(value[0], value[1]))

    def put_value_op(self, id_, i0, i1, i2, i3, value, op=OP_SET):
        """A value command: a double (set, inc, immediately), a bool (touch, indicate), None
        (reset)."""
        if value is None:
            _REC_NONE.pack_into(self.buf, self._reserve(24), id_, T_NONE, op, i0, i1, i2, i3, 0, 0)
        elif isinstance(value, bool):
            _REC_I64.pack_into(self.buf, self._reserve(24), id_, T_BOOL, op, i0, i1, i2, i3, 0, 1 if value else 0)
        else:
            _REC_F64.pack_into(self.buf, self._reserve(24), id_, T_F64, op, i0, i1, i2, i3, 0, value)

    def put(self, id_, i0, i1, i2, i3, value=None):
        """A record of any ID, the value written by the kind of the ID."""
        _PUTTERS[KINDS[id_]](self, id_, i0, i1, i2, i3, value)

    # By key (key(): the ID and the four index slots in one int, the change tracking's key of
    # TYPED_PROTOCOL_PLAN §2.4): the indices are packed as one u64, the key is not unpacked

    def put_key_none(self, k, value=None):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _KREC_I64.pack_into(self.buf, p, k & 0xFFFF, 0, 0, k >> 16, 0, 0)
        self.size = p + 24

    def put_key_bool(self, k, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _KREC_I64.pack_into(self.buf, p, k & 0xFFFF, 1, 0, k >> 16, 0, 1 if value else 0)
        self.size = p + 24

    def put_key_i32(self, k, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _KREC_I64.pack_into(self.buf, p, k & 0xFFFF, 2, 0, k >> 16, 0, value)
        self.size = p + 24

    def put_key_f64(self, k, value):
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        _KREC_F64.pack_into(self.buf, p, k & 0xFFFF, 3, 0, k >> 16, 0, value)
        self.size = p + 24

    def put_key_text(self, k, value):
        data = ("" if value is None else str(value)).encode("utf-8")
        n = len(data)
        p = self._reserve(24 + ((n + 7) & ~7))
        _KREC_I64.pack_into(self.buf, p, k & 0xFFFF, T_TEXT, 0, k >> 16, n, 0)
        self.buf[p + 24:self.size] = data + _PAD[n & 7]

    def put_key_color(self, k, value):
        """value: (r, g, b) 0..255, None: no color."""
        p = self.size
        if p + 24 > len(self.buf):
            self.buf.extend(bytes(len(self.buf)))
        if value is None:
            _KREC_COLOR.pack_into(self.buf, p, k & 0xFFFF, T_COLOR, 0, k >> 16, 0, 0, 0, 0, 0)
        else:
            r, g, b = value
            c = lambda v: 0 if v < 0 else 255 if v > 255 else int(v)  # noqa: E731
            _KREC_COLOR.pack_into(self.buf, p, k & 0xFFFF, T_COLOR, 0, k >> 16, 0, c(r), c(g), c(b), 1)
        self.size = p + 24

    def put_key(self, k, value=None):
        """A record of any ID by key, the value written by the kind of the ID."""
        id_ = k & 0xFFFF
        put = KEY_PUTTERS.get(KINDS[id_])
        if put is None:
            _PUTTERS[KINDS[id_]](self, id_, k >> 16 & 0xFFFF, k >> 32 & 0xFFFF, k >> 48 & 0xFFFF, k >> 64 & 0xFFFF, value)
        else:
            put(self, k, value)


_KREC_I64 = struct.Struct("<HBBQIq")
_KREC_F64 = struct.Struct("<HBBQId")
_KREC_COLOR = struct.Struct("<HBBQI4B4x")

KEY_PUTTERS = {
    "none": _WriterBase.put_key_none, "bool": _WriterBase.put_key_bool, "i32": _WriterBase.put_key_i32,
    "f64": _WriterBase.put_key_f64, "text": _WriterBase.put_key_text, "color": _WriterBase.put_key_color,
}
"""The writers by key (put_key_*) by value kind: the inline values and texts."""


def key_id(k: int) -> int:
    """The ID of a key."""
    return k & 0xFFFF


def key_indices(k: int) -> tuple:
    """The four index slots of a key."""
    return (k >> 16 & 0xFFFF, k >> 32 & 0xFFFF, k >> 48 & 0xFFFF, k >> 64 & 0xFFFF)


def record_key(head) -> int:
    """The key of a record head (the h of a Record, the heads of _records)."""
    return head[0] | head[3] << 16 | head[4] << 32 | head[5] << 48 | head[6] << 64


_PUTTERS = {
    "none": lambda w, id_, i0, i1, i2, i3, value: w.put_none(id_, i0, i1, i2, i3),
    "bool": _WriterBase.put_bool, "i32": _WriterBase.put_i32, "f64": _WriterBase.put_f64,
    "text": _WriterBase.put_text, "text_list": _WriterBase.put_text_list, "color": _WriterBase.put_color,
    "notes": _WriterBase.put_notes, "step": _WriterBase.put_step, "value_op": _WriterBase.put_value_op,
    "pair": _WriterBase.put_pair, "int_double": _WriterBase.put_int_double,
}
"""The writers by value kind."""

# The index accessors of the views
_i0 = property(lambda s: s.h[3])
_i1 = property(lambda s: s.h[4])
_i2 = property(lambda s: s.h[5])
_i3 = property(lambda s: s.h[6])
_s0 = property(lambda s: s.h[3] - 0x10000 if s.h[3] & 0x8000 else s.h[3])
_s1 = property(lambda s: s.h[4] - 0x10000 if s.h[4] & 0x8000 else s.h[4])
_s2 = property(lambda s: s.h[5] - 0x10000 if s.h[5] & 0x8000 else s.h[5])
_s3 = property(lambda s: s.h[6] - 0x10000 if s.h[6] & 0x8000 else s.h[6])


def _part(slot: int, shift: int, bits: int) -> property:
    mask = (1 << bits) - 1
    return property(lambda s: (s.h[3 + slot] >> shift) & mask)


def _value(kind: str) -> property:
    get = GETTERS[kind]
    return property(lambda s: get(s.b, s.p, s.h[7], s.h[1]))


def signed(v: int) -> int:
    """An index slot as an i16."""
    return v - 0x10000 if v & 0x8000 else v
