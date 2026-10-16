"""Strict JSON framing and replay guards for local desktop adapters.

This module supplies transport primitives, not an authority or listening service.
"""
import hashlib
import json
import math
import struct
from collections import OrderedDict

MAX_FRAME = 1024 * 1024


class ProtocolError(ValueError):
    pass


def _pairs(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ProtocolError('Duplicate object key')
        value[key] = item
    return value


def _finite(value):
    if isinstance(value,str) and any(0xD800<=ord(c)<=0xDFFF for c in value):
        raise ProtocolError('Unpaired Unicode surrogate')
    if isinstance(value, float) and not math.isfinite(value):
        raise ProtocolError('Nonfinite number')
    if isinstance(value, dict):
        if any(not isinstance(key,str) for key in value):
            raise ProtocolError('Object keys must be strings')
        for item in value.values():
            _finite(item)
    elif isinstance(value, list):
        for item in value:
            _finite(item)


def loads(data):
    try:
        if isinstance(data, bytes):
            data = data.decode('utf-8', errors='strict')
        value = json.loads(data, object_pairs_hook=_pairs,
                           parse_constant=lambda _: (_ for _ in ()).throw(ProtocolError('Nonfinite number')))
        _finite(value)
        return value
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as exc:
        raise ProtocolError('Invalid JSON payload') from exc


def encode(value):
    try:
        _finite(value)
        raw = json.dumps(value, allow_nan=False, ensure_ascii=False,
                         separators=(',', ':'), sort_keys=True).encode('utf-8')
    except (TypeError, ValueError, RecursionError) as exc:
        raise ProtocolError('Unserialisable payload') from exc
    if not raw or len(raw) > MAX_FRAME:
        raise ProtocolError('Frame exceeds permitted size')
    return struct.pack('!I', len(raw)) + raw


class Decoder:
    """Incremental decoder, bounded before accepting body bytes."""
    def __init__(self):
        self.buffer = bytearray()
        self.expected = None
        self.failed = False

    def feed(self, chunk):
        if self.failed:
            raise ProtocolError('Decoder is closed after invalid input')
        if not isinstance(chunk, bytes):
            raise TypeError('Transport data must be bytes')
        results = []
        offset = 0
        try:
            while offset < len(chunk):
                limit = 4 if self.expected is None else self.expected
                take = min(limit - len(self.buffer), len(chunk) - offset)
                self.buffer.extend(chunk[offset:offset + take])
                offset += take
                if len(self.buffer) != limit:
                    continue
                if self.expected is None:
                    self.expected = struct.unpack('!I', self.buffer)[0]
                    self.buffer.clear()
                    if not 0 < self.expected <= MAX_FRAME:
                        raise ProtocolError('Invalid frame length')
                else:
                    results.append(loads(bytes(self.buffer)))
                    self.buffer.clear()
                    self.expected = None
            return results
        except (ProtocolError, ValueError):
            self.failed = True
            self.buffer.clear()
            raise

    def finish(self):
        if self.buffer or self.expected is not None:
            self.failed = True
            raise ProtocolError('Truncated frame')


class ReplayGuard:
    """Bounded, peer-scoped request fingerprints with explicit expiry.

    Results are retained only in memory; an adapter restart requires reconciliation.
    A capacity error fails closed rather than forgetting an unexpired request.
    """
    def __init__(self, clock, retention=600, capacity=4096):
        self.clock = clock
        self.retention = retention
        self.capacity = capacity
        self.entries = OrderedDict()

    def remember(self, epoch, peer, request_id, payload):
        if not all(isinstance(v, str) and v for v in (epoch, peer, request_id)):
            raise ProtocolError('Request identity is required')
        now = self.clock()
        for key, (expiry, _) in list(self.entries.items()):
            if expiry <= now:
                del self.entries[key]
        key = (epoch, peer, request_id)
        digest = hashlib.sha256(encode(payload)).digest()
        if key in self.entries:
            if self.entries[key][1] != digest:
                raise ProtocolError('Request identity reused with different payload')
            return False
        if len(self.entries) >= self.capacity:
            raise ProtocolError('Request retention capacity reached')
        self.entries[key] = (now + self.retention, digest)
        return True


class RevisionCursor:
    """Stop revision-sensitive work after gaps, epoch changes or stale snapshots."""
    def __init__(self):
        self.epoch = None
        self.sequence = None
        self.revision = None
        self.ready = False
        self.observed_epoch = None
        self.sequence_floor = 0
        self.revision_floor = 0

    def snapshot(self, epoch, sequence, revision):
        self._numbers(sequence, revision)
        if not isinstance(epoch, str) or not epoch:
            raise ProtocolError('Session epoch is required')
        if self.observed_epoch == epoch and (sequence < self.sequence_floor or revision < self.revision_floor):
            raise ProtocolError('Snapshot predates observed high-water mark')
        if self.observed_epoch is not None and epoch != self.observed_epoch:
            raise ProtocolError('Snapshot does not match observed session epoch')
        if self.epoch == epoch and self.sequence is not None and (sequence < self.sequence or revision < self.revision):
            raise ProtocolError('Snapshot predates observed event sequence')
        self.epoch, self.sequence, self.revision = epoch, sequence, revision
        self.ready = True
        self.observed_epoch = epoch
        self.sequence_floor, self.revision_floor = sequence, revision

    @staticmethod
    def _numbers(sequence, revision):
        if any(type(v) is not int or v < 0 for v in (sequence, revision)):
            raise ProtocolError('Nonnegative integer sequence and revision required')

    def event(self, epoch, sequence, revision):
        self._numbers(sequence, revision)
        if self.observed_epoch != epoch:
            self.observed_epoch = epoch
            self.sequence_floor = sequence
            self.revision_floor = revision
        else:
            self.sequence_floor = max(sequence,self.sequence_floor)
            self.revision_floor = max(revision,self.revision_floor)
        if not self.ready or epoch != self.epoch or sequence != self.sequence + 1 or revision < self.revision:
            self.ready = False
            raise ProtocolError('Snapshot reconciliation required')
        self.sequence, self.revision = sequence, revision
