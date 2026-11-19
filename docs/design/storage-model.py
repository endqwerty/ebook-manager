"""Scratch model of the proposed library storage contract (docs/design/library-storage.md).

Design B (selected): every device owns one append-only change log (one allocator of change IDs).
The local database is a cache of (all logs + own unpublished outbox). Sidecars are
derived bytes. Merged state is a pure function of the set of changes.

Also models two rejected designs as negative controls:
  A: one hub owns the library; other devices queue edits for it.
  C: per-book sidecar file is the authority; last whole-file write wins.
And one rule ablation: reusing a replica id after a restore/rebuild.
"""

import json
import random
import sys
from itertools import count

FIELDS = ["title", "series", "rating"]
RELAY = True
TAGS = ["a", "b", "c", "d"]


def leq(vv1, vv2):
    return all(vv2.get(k, 0) >= v for k, v in vv1.items())


class Change:
    __slots__ = ("book", "deps", "lam", "op", "origin", "rid", "seq")

    def __init__(self, rid, seq, deps, lam, origin, book, op):
        self.rid, self.seq, self.deps, self.lam = rid, seq, dict(deps), lam
        self.origin, self.book, self.op = origin, book, op

    @property
    def id(self):
        return (self.rid, self.seq)

    def before(self, other):  # self happened-before other
        return other.deps.get(self.rid, 0) >= self.seq

    def key(self):
        return json.dumps(
            [self.rid, self.seq, self.deps, self.lam, self.origin, self.book, self.op],
            sort_keys=True,
        )


def merge(changes):
    """Pure function: set of changes -> (state, conflicts, notices)."""
    by_book = {}
    for c in changes.values():
        by_book.setdefault(c.book, []).append(c)
    state, conflicts, notices = {}, set(), set()
    for book, cs in sorted(by_book.items()):
        if not any(c.op[0] == "create" for c in cs):
            continue
        deletes = [c for c in cs if c.op[0] == "delete"]
        others = [c for c in cs if c.op[0] != "delete"]
        dead = any(all(o.before(d) for o in others) for d in deletes)
        for d in deletes:
            if any(not o.before(d) and not d.before(o) for o in others):
                notices.add((book, "edited-while-deleted"))
        if dead:
            continue
        rec = {}
        for f in FIELDS:
            sets = [c for c in cs if c.op[0] == "set" and c.op[1] == f]
            tops = [c for c in sets if not any(c.before(o) for o in sets)]
            if not tops:
                continue
            users = [c for c in tops if c.origin == "user"]
            pool = users or tops
            win = max(pool, key=lambda c: (c.lam, c.rid))
            rec[f] = win.op[2]
            if len({c.op[2] for c in users}) > 1:
                conflicts.add((book, f, tuple(sorted(str(c.op[2]) for c in users))))
        adds = [c for c in cs if c.op[0] == "tag_add"]
        rms = [c for c in cs if c.op[0] == "tag_rm"]
        tags = set()
        for a in adds:
            if not any(r.op[1] == a.op[1] and a.before(r) for r in rms):
                tags.add(a.op[1])
        rec["tags"] = sorted(tags)
        state[book] = rec
    return state, conflicts, notices


def vv_of(changes):
    vv = {}
    for rid, seq in changes:
        vv[rid] = max(vv.get(rid, 0), seq)
    return vv


def sidecar_bytes(changes, book):
    state, _, _ = merge(changes)
    rel = {k: v for k, v in changes.items() if v.book == book}
    body = {
        "book": book,
        "covers": vv_of(rel),
        "record": state.get(book),
        "deleted": book not in state,
    }
    return json.dumps(body, sort_keys=True, separators=(",", ":")).encode()


class Shared:
    """Shared folder (SYNC-22) / portable store: one directory per replica. Peers may relay accepted changes."""

    def __init__(self):
        self.logs = {}  # rid -> list of change keys, index = seq-1
        self.objs = {}

    def append(self, c):
        lst = self.logs.setdefault(c.rid, [])
        if len(lst) >= c.seq:
            if lst[c.seq - 1] != c.key():
                raise ForkError(c.id)
            return
        assert len(lst) == c.seq - 1, "gap"
        lst.append(c.key())
        self.objs[c.id] = c


class ForkError(Exception):
    pass


class Device:
    def __init__(self, name, shared, clock_skew):
        self.name, self.shared, self.skew = name, shared, clock_skew
        self.generations = count(1)
        self.new_replica()
        self.db = {}  # change id -> Change (cache)
        self.outbox = []
        self.lam = 0
        self.saved, self.portable = set(), set()

    def new_replica(self):
        self.rid = f"{self.name}-{next(self.generations)}"
        self.seq = 0

    def ingest(self, c):
        if c.id in self.db:
            if self.db[c.id].key() != c.key():
                raise ForkError(c.id)
            return
        self.db[c.id] = c
        self.lam = max(self.lam, c.lam)

    def edit(self, book, op, origin="user"):
        self.seq += 1
        # Lamport counter seeded with a skewed wall clock: ordering must not depend on it.
        self.lam = max(self.lam + 1, self.skew)
        c = Change(self.rid, self.seq, vv_of(self.db), self.lam, origin, book, op)
        self.db[c.id] = c  # one transaction: domain rows + change + outbox row
        self.outbox.append(c.id)
        self.saved.add(c.id)
        return c

    def publish(self, crash_after=None):
        n = 0
        while self.outbox:
            if crash_after is not None and n >= crash_after:
                return  # crash mid-publish: remaining outbox rows survive in the db
            cid = self.outbox[0]
            self.shared.append(self.db[cid])  # temp file + rename + fsync, then
            self.outbox.pop(0)  # mark the outbox row done
            self.portable.add(cid)
            n += 1
        if RELAY:
            # Publish every held change the store lacks, including changes from
            # replicas that no longer exist. Immutable and identical bytes, so a
            # second writer of the same change is harmless.
            for c in sorted(self.db.values(), key=lambda c: c.id):
                if len(self.shared.logs.get(c.rid, [])) == c.seq - 1:
                    self.shared.append(c)

    def read_folder(self):
        for rid, lst in self.shared.logs.items():
            for i in range(len(lst)):
                self.ingest(self.shared.objs[(rid, i + 1)])

    def lose_db(self, rotate=True):
        """Database deleted or corrupt: rebuild from the portable store only."""
        self.db, self.outbox = {}, []
        self.read_folder()
        if rotate:
            self.new_replica()
        else:
            own = self.shared.logs.get(self.rid, [])
            self.seq = len(own)

    def restore_backup(self, snapshot, rotate=True):
        self.db = dict(snapshot["db"])
        self.outbox = list(snapshot["outbox"])
        self.lam = snapshot["lam"]
        if rotate:
            # Unpublished outbox rows from the backup were never seen by anyone under the
            # new id; republish them under the old id only if the log does not already
            # hold that seq. Simplest safe rule: keep them under the old id.
            self.new_replica()
        else:
            self.rid, self.seq = snapshot["rid"], snapshot["seq"]
        self.read_folder()

    def snapshot(self):
        return {
            "db": dict(self.db),
            "outbox": list(self.outbox),
            "lam": self.lam,
            "rid": self.rid,
            "seq": self.seq,
        }


def lan_exchange(a, b):
    for c in list(a.db.values()):
        b.ingest(c)
    for c in list(b.db.values()):
        a.ingest(c)


def random_op(rng, dev, books):
    r = rng.random()
    if r < 0.1 or not books:
        b = f"b{len(books)}-{dev.rid}"
        books.append(b)
        return dev.edit(b, ["create"])
    book = rng.choice(books)
    if r < 0.55:
        f = rng.choice(FIELDS)
        origin = "auto" if rng.random() < 0.2 else "user"
        return dev.edit(book, ["set", f, rng.randint(0, 3)], origin)
    if r < 0.75:
        return dev.edit(book, ["tag_add", rng.choice(TAGS), dev.seq + 1])
    if r < 0.85:
        return dev.edit(book, ["tag_rm", rng.choice(TAGS)])
    if r < 0.93:
        return dev.edit(book, ["delete"])
    return dev.edit(book, ["restore"])


def run_b(seed, rotate=True, steps=120):
    rng = random.Random(seed)
    shared = Shared()
    devs = [Device(n, shared, rng.randint(0, 50)) for n in ("desk", "phone", "tab")]
    books, backups, events = [], {}, []
    for _ in range(steps):
        d = rng.choice(devs)
        x = rng.random()
        if x < 0.55:
            random_op(rng, d, books)
        elif x < 0.65:
            d.publish(crash_after=rng.choice([None, 0, 1, 2]))
        elif x < 0.75:
            d.read_folder()
        elif x < 0.85:
            lan_exchange(d, rng.choice(devs))
        elif x < 0.90:
            backups[d.name] = d.snapshot()
        elif x < 0.95:
            # DB lost. Only changes reported "portable copy up to date" must survive.
            portable_before = set(d.portable)
            d.lose_db(rotate)
            assert portable_before <= set(d.db), "portable change lost"
            events.append("lose")
        elif d.name in backups:
            d.restore_backup(backups[d.name], rotate)
            events.append("restore")
    # Quiesce: everyone publishes, everyone reads, everyone LAN-syncs.
    for d in devs:
        d.publish()
    for d in devs:
        d.read_folder()
    for a in devs:
        for b in devs:
            lan_exchange(a, b)
    # Invariants
    states = [merge(d.db) for d in devs]
    assert all(s == states[0] for s in states), "replicas diverged"
    for b in set(books):
        bytes0 = sidecar_bytes(devs[0].db, b)
        assert all(sidecar_bytes(d.db, b) == bytes0 for d in devs), (
            "sidecar bytes differ"
        )
    # Rebuild from the portable store alone gives the same state.
    fresh = Device("fresh", shared, 0)
    fresh.read_folder()
    assert merge(fresh.db) == states[0], "rebuild differs"
    # Every user edit that is still causally current is either shown or in a conflict record.
    st, conflicts, _ = states[0]
    allc = devs[0].db
    for c in allc.values():
        if c.op[0] != "set" or c.origin != "user" or c.book not in st:
            continue
        same = [o for o in allc.values() if o.book == c.book and o.op[:2] == c.op[:2]]
        if any(c.before(o) for o in same):
            continue  # knowingly superseded
        shown = st[c.book].get(c.op[1]) == c.op[2]
        in_conflict = any(
            k[0] == c.book and k[1] == c.op[1] and str(c.op[2]) in k[2]
            for k in conflicts
        )
        assert shown or in_conflict, f"user edit silently lost {c.id}"
    return events


def check_targeted():
    sh = Shared()
    a, b = Device("a", sh, 1000), Device("b", sh, 0)  # a's clock far ahead
    a.edit("x", ["create"])
    a.publish()
    b.read_folder()
    # Causal later edit beats an earlier one even with a slower clock.
    a.edit("x", ["set", "title", "old"])
    lan_exchange(a, b)
    b.edit("x", ["set", "title", "new"])
    lan_exchange(a, b)
    assert merge(a.db)[0]["x"]["title"] == "new", "clock skew decided order"
    # Concurrent user edits: one shown, both kept as a conflict.
    a.edit("x", ["set", "series", "S1"])
    b.edit("x", ["set", "series", "S2"])
    lan_exchange(a, b)
    _, conf, _ = merge(a.db)
    assert any(k[:2] == ("x", "series") for k in conf)
    # User edit beats a concurrent automated fetch, with no conflict record.
    a.edit("x", ["set", "rating", 5])
    b.edit("x", ["set", "rating", 1], origin="auto")
    lan_exchange(a, b)
    st, conf, _ = merge(a.db)
    assert st["x"]["rating"] == 5 and not any(k[:2] == ("x", "rating") for k in conf)
    # Concurrent tag adds both survive; a remove only removes what it saw.
    a.edit("x", ["tag_add", "p", 1])
    b.edit("x", ["tag_add", "q", 1])
    lan_exchange(a, b)
    b.edit("x", ["tag_rm", "p"])
    a.edit("x", ["tag_add", "p", 2])
    lan_exchange(a, b)
    assert merge(a.db)[0]["x"]["tags"] == ["p", "q"]
    # Edit beats a concurrent delete, with a notice; a later delete sticks.
    a.edit("x", ["delete"])
    b.edit("x", ["set", "title", "kept"])
    lan_exchange(a, b)
    st, _, notes = merge(a.db)
    assert "x" in st and ("x", "edited-while-deleted") in notes
    a.edit("x", ["delete"])
    lan_exchange(a, b)
    a.publish()
    b.publish()
    assert "x" not in merge(b.db)[0]
    # Rebuild after deletion does not bring the book back.
    b.lose_db()
    assert "x" not in merge(b.db)[0]
    # Hand edit of a sidecar: diff against the state at the sidecar's 'covers' vector,
    # emitted with that vector as deps, so it is concurrent with newer edits, not newer.
    a.edit("y", ["create"])
    a.edit("y", ["set", "title", "T0"])
    a.publish()
    b.read_folder()
    side = json.loads(sidecar_bytes(a.db, "y"))
    side["record"]["title"] = "hand"
    a.edit("y", ["set", "title", "T1"])  # newer app edit the hand-editor never saw
    base = {k: v for k, v in a.db.items() if leq({k[0]: k[1]}, side["covers"])}
    base_state = merge(base)[0]["y"]
    assert base_state["title"] != side["record"]["title"]
    a.seq += 1
    a.lam += 1
    patch = Change(
        a.rid, a.seq, side["covers"], a.lam, "user", "y", ["set", "title", "hand"]
    )
    a.db[patch.id] = patch
    st, conf, _ = merge(a.db)
    assert any(k[:2] == ("y", "title") for k in conf), (
        "hand edit must surface as a conflict, not overwrite"
    )
    print("targeted cases: ok")


def check_design_c():
    """Rejected design C: sidecar file is the authority, whole-file last write wins."""
    folder = {"x": {"title": "t", "rating": 0}}
    phone = dict(folder["x"])
    desk = dict(folder["x"])
    phone["title"] = "phone edit"
    desk["rating"] = 5  # different fields, offline
    folder["x"] = phone
    folder["x"] = desk  # the sync tool keeps the later file
    lost = folder["x"]["title"] != "phone edit"
    print(f"design C loses a non-conflicting edit: {lost}")
    return lost


def check_design_a():
    """Rejected design A: hub owns writes. Hub off; phone and tablet on the same LAN."""
    hub_online = False
    phone_queue = [("x", "title", "phone")]
    tablet_view = {}
    if hub_online:
        for b, f, v in phone_queue:
            tablet_view[f] = v
    a_ok = tablet_view.get("title") == "phone"
    sh = Shared()
    p, t = Device("p", sh, 0), Device("t", sh, 0)
    p.edit("x", ["create"])
    p.edit("x", ["set", "title", "phone"])
    lan_exchange(p, t)
    b_ok = merge(t.db)[0]["x"]["title"] == "phone"
    print(f"hub offline, phone->tablet edit visible: design A {a_ok}, design B {b_ok}")
    return (not a_ok) and b_ok


def ablation_no_rotation(trials):
    hits = 0
    for s in range(trials):
        try:
            run_b(s, rotate=False)
        except (ForkError, AssertionError):
            hits += 1
    return hits


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("Run with assertions enabled to check the storage model.")
    trials = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    check_targeted()
    ok_c = check_design_c()
    ok_a = check_design_a()
    stats = {"lose": 0, "restore": 0}
    for s in range(trials):
        for e in run_b(s):
            stats[e] += 1
    print(
        f"design B randomized: {trials} runs passed; db losses {stats['lose']}, restores {stats['restore']}"
    )
    bad = ablation_no_rotation(trials)
    print(
        f"ablation, replica id kept after rebuild/restore: {bad}/{trials} runs fork or fail"
    )
    RELAY = False
    norelay = 0
    for s in range(trials):
        try:
            run_b(s)
        except AssertionError:
            norelay += 1
    print(
        f"ablation, only the author publishes its changes: {norelay}/{trials} runs lose data on rebuild"
    )
    assert ok_c and ok_a and bad > 0 and norelay > 0
    print("ALL CHECKS PASSED")
