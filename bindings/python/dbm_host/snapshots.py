# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The host's snapshots (HOST_BRIDGE_GUIDE §4.14), simulated: a set of stored states which the
host numbers, names, colours and keeps (zynthian's ZS3). Opt-in (``SimDaw.snapshots``), so the
default host handles no snapshot command, as Bitwig.

Two kinds of hosts:

- **appendOnly** (the default): a dense list 0..count-1; a store at index = count appends a new
  one, a remove closes the gap (the later ones move down).
- **sparse** (``append_only=False``): slots with gaps, slot i = the snapshot of program change i
  (zynthian); count is the number of slots (``slot_count``, zynthian 128; the window reports the
  first 64), a store goes into the given slot.

The reference ``CommandHandler`` and ``Reporter`` use a DAW attribute named ``snapshots`` (an
instance of this class) whenever it exists: they register the snapshot commands and report it.
A host which subclasses them with its own snapshot object names that attribute differently
(zynthian: ``zs3_snapshots``) and handles the commands itself.

The reports (change-only through the reporter's ``put``): ``/snapshot/appendOnly``,
``/snapshot/count``, ``/snapshot/active`` and for each slot of the window ``exists``, ``name``
and ``color``."""

from dataclasses import dataclass

from drivenbymoss import protocol

from .reporter import M

BANK_SIZE = 64
"""The slots of the bank window (offset 0)."""

SNAPSHOT_COMMANDS = (protocol.Cmd.SNAPSHOT_LOAD, protocol.Cmd.SNAPSHOT_STORE, protocol.Cmd.SNAPSHOT_REMOVE)
"""The commands of the snapshots."""

_NO_COLOR = (-1, -1, -1)


@dataclass
class Snapshot:
    name: str
    color: tuple[int, int, int] | None = None


class Snapshots:
    """The snapshots of the simulated host."""

    def __init__(self, append_only: bool = True, names: dict[int, str] | list[str] | None = None, slot_count: int = BANK_SIZE):
        """``names``: the stored ones, a list (appendOnly) or a dict slot → name (sparse);
        ``slot_count``: the slots of a sparse host (the reported count, may exceed the window:
        zynthian 128)."""
        self.append_only = append_only
        self.slots: list[Snapshot | None] = [None] * slot_count
        self.items: list[Snapshot] = []
        """The dense list (appendOnly)."""
        self.active = -1
        if isinstance(names, dict):
            for slot, name in names.items():
                self.slots[slot] = Snapshot(name)
        elif names:
            self.items = [Snapshot(name) for name in names]
        self._created = len(names or ())

    @property
    def count(self) -> int:
        """The reported count: appendOnly the stored ones, sparse the whole window."""
        return len(self.items) if self.append_only else len(self.slots)

    def get(self, index: int) -> Snapshot | None:
        if self.append_only:
            return self.items[index] if 0 <= index < len(self.items) else None
        return self.slots[index] if 0 <= index < len(self.slots) else None

    def load(self, index: int) -> bool:
        """Recall a stored snapshot (it becomes the active one)."""
        if self.get(index) is None:
            return False
        self.active = index
        return True

    def store(self, index: int) -> bool:
        """Store the current state into the slot (overwrite an occupied one; appendOnly: index =
        count appends a new one)."""
        if self.get(index) is None:
            self._created += 1
            snapshot = Snapshot(f"Snapshot {self._created}")
            if self.append_only:
                if index != len(self.items):
                    return False
                self.items.append(snapshot)
            elif 0 <= index < len(self.slots):
                self.slots[index] = snapshot
            else:
                return False
        self.active = index
        return True

    def remove(self, index: int) -> bool:
        if self.get(index) is None:
            return False
        if self.append_only:
            del self.items[index]
            if self.active > index:
                self.active -= 1
        else:
            self.slots[index] = None
        if self.active == index:
            self.active = -1
        return True

    def report(self, put) -> None:
        """Report the snapshots (change-only through ``put``)."""
        put(M.SNAPSHOT_APPEND_ONLY, self.append_only)
        put(M.SNAPSHOT_COUNT, self.count)
        put(M.SNAPSHOT_ACTIVE, self.active)
        for slot in range(BANK_SIZE):
            snapshot = self.get(slot)
            base = slot << 16
            put(M.SNAPSHOT_EXISTS | base, snapshot is not None)
            if snapshot is not None:
                put(M.SNAPSHOT_NAME | base, snapshot.name)
                put(M.SNAPSHOT_COLOR | base, snapshot.color or _NO_COLOR)
