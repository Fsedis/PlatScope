"""Проверки восстановления двоичных снимков и границ отсутствующих данных."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import zlib

from inspect_memory_archive import MemoryArchive


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="platscope-archive-test-")
        self.root = Path(self.temp.name)
        (self.root / "session.json").write_text(json.dumps({"format": 1, "compression": "zlib"}))
        self.pack = bytearray()

    def tearDown(self):
        self.temp.cleanup()

    def block(self, raw):
        packed = zlib.compress(raw)
        reference = {"sha256": hashlib.sha256(raw).hexdigest(), "offset": len(self.pack), "compressed_size": len(packed), "size": len(raw)}
        self.pack.extend(packed)
        (self.root / "blocks.bin").write_bytes(self.pack)
        return reference

    def snapshot(self, sequence, changed, removed=(), complete=True):
        (self.root / f"snapshot-{sequence:06d}.json").write_text(json.dumps({"format": 1, "sequence": sequence, "parent": sequence - 1 if sequence > 1 else None, "changed": changed, "removed": list(removed), "complete": complete}))

    def test_delta_reconstruction_and_cross_block_pointer(self):
        self.snapshot(1, {4096: self.block(b"ABCD"), 4100: self.block(b"EFGH")})
        self.snapshot(2, {4100: self.block(b"\x00\xffGH")})
        before = MemoryArchive(self.root, 1)
        after = MemoryArchive(self.root, 2)
        self.assertEqual(before.read(4098, 6), b"CDEFGH")
        self.assertEqual(after.read(4098, 6), b"CD\x00\xffGH")
        self.assertEqual(list(after.find(b"D\x00\xff")), [4099])

    def test_removed_or_partial_regions_never_reuse_stale_bytes(self):
        self.snapshot(1, {4096: self.block(b"ABCD"), 4100: self.block(b"EFGH")})
        self.snapshot(2, {4100: self.block(b"EF")}, removed=[4096], complete=False)
        latest = MemoryArchive(self.root)
        with self.assertRaises(ValueError): latest.read(4096, 1)
        with self.assertRaises(ValueError): latest.read(4100, 4)
        self.assertEqual(latest.read(4100, 2), b"EF")
        self.assertEqual(list(latest.find(b"DE")), [])

    def test_corruption_is_detected(self):
        reference = self.block(b"abcd")
        reference["sha256"] = "0" * 64
        self.snapshot(1, {100: reference})
        with self.assertRaisesRegex(ValueError, "SHA-256"): MemoryArchive(self.root).read(100, 4)

    def test_interrupted_index_does_not_hide_last_complete_index(self):
        self.snapshot(1, {100: self.block(b"abc")})
        (self.root / "snapshot-000002.partial").write_bytes(b'{"changed":')
        self.assertEqual(MemoryArchive(self.root).read(100, 3), b"abc")

    def test_cyclic_parent_is_rejected(self):
        self.snapshot(1, {})
        path = self.root / "snapshot-000001.json"
        value = json.loads(path.read_text())
        value["parent"] = 1
        path.write_text(json.dumps(value))
        with self.assertRaises(ValueError): MemoryArchive(self.root)


if __name__ == "__main__":
    unittest.main()
