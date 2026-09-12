"""Локальный разбор двоичной записи PlatScope без запуска игры. Python 3.10+."""
from __future__ import annotations

import argparse
import bisect
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import sys
import zlib


class MemoryArchive:
    def __init__(self, directory: Path, sequence: int | None = None):
        self.directory = directory
        self.session = json.loads((directory / "session.json").read_text(encoding="utf-8"))
        if self.session.get("format") != 1 or self.session.get("compression") != "zlib":
            raise ValueError("Неподдерживаемый формат записи")
        self.snapshots = sorted(directory.glob("snapshot-*.json"))
        if not self.snapshots:
            raise ValueError("В папке нет завершённых индексов снимков")
        self.sequence = sequence if sequence is not None else int(self.snapshots[-1].stem.split("-")[1])
        self.blocks: dict[int, dict] = {}
        parent = self.sequence
        chain = []
        seen = set()
        while parent is not None:
            if not isinstance(parent, int) or parent < 1 or parent in seen:
                raise ValueError("Повреждена цепочка снимков")
            seen.add(parent)
            snapshot = json.loads((directory / f"snapshot-{parent:06d}.json").read_text(encoding="utf-8"))
            if snapshot.get("format") != 1 or snapshot.get("sequence") != parent:
                raise ValueError("Неверная версия или номер снимка")
            chain.append(snapshot)
            next_parent = snapshot.get("parent")
            if next_parent is not None and (not isinstance(next_parent, int) or next_parent >= parent):
                raise ValueError("Некорректный родительский снимок")
            parent = next_parent
        self.snapshot = chain[0]
        for snapshot in reversed(chain):
            for address in snapshot["removed"]:
                self.blocks.pop(int(address), None)
            for address, block in snapshot["changed"].items():
                address = int(address)
                if address < 0 or not 0 < block["size"] <= 256 * 1024:
                    raise ValueError("Некорректная граница блока")
                self.blocks[address] = block
        self.addresses = sorted(self.blocks)
        end = 0
        for address in self.addresses:
            if address < end:
                raise ValueError("Перекрывающиеся блоки снимка")
            end = address + self.blocks[address]["size"]

    @lru_cache(maxsize=8)
    def block(self, address: int) -> bytes:
        reference = self.blocks[address]
        offset, length, size = reference["offset"], reference["compressed_size"], reference["size"]
        pack = self.directory / "blocks.bin"
        if offset < 0 or not 0 < length <= 512 * 1024 or offset + length > pack.stat().st_size:
            raise ValueError(f"Блок 0x{address:x} выходит за границы файла")
        with pack.open("rb") as file:
            file.seek(offset)
            compressed = file.read(length)
        decoder = zlib.decompressobj()
        raw = decoder.decompress(compressed, size + 1)
        if len(raw) != size or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:
            raise ValueError(f"Повреждено сжатие блока 0x{address:x}")
        if hashlib.sha256(raw).hexdigest() != reference["sha256"]:
            raise ValueError(f"Не совпадает SHA-256 блока 0x{address:x}")
        return raw

    def read(self, address: int, length: int) -> bytes:
        if address < 0 or not 1 <= length <= 16 * 1024 * 1024:
            raise ValueError("Длина чтения должна быть от 1 байта до 16 МиБ")
        result = bytearray()
        while len(result) < length:
            cursor = address + len(result)
            index = bisect.bisect_right(self.addresses, cursor) - 1
            if index < 0:
                raise ValueError(f"Адрес 0x{cursor:x} не сохранён")
            base = self.addresses[index]
            offset = cursor - base
            if offset >= self.blocks[base]["size"]:
                raise ValueError(f"Адрес 0x{cursor:x} не сохранён; старые данные не подставляются")
            raw = self.block(base)
            result.extend(raw[offset:offset + length - len(result)])
        return bytes(result)

    def find(self, needle: bytes, limit: int = 100):
        if not 1 <= len(needle) <= 65536 or limit < 1:
            raise ValueError("Шаблон должен содержать 1–65536 байт, предел результатов — положительное число")
        tail = b""
        previous_end = None
        found = 0
        for address in self.addresses:
            raw = self.block(address)
            if previous_end != address:
                tail = b""
            window = tail + raw
            start = 0
            while (match := window.find(needle, start)) >= 0:
                yield address - len(tail) + match
                found += 1
                if found >= limit:
                    return
                start = match + 1
            tail = window[-(len(needle) - 1):] if len(needle) > 1 else b""
            previous_end = address + len(raw)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="Папка warframe-binary-...")
    parser.add_argument("--snapshot", type=int, help="Номер снимка; по умолчанию последний")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--list", action="store_true", help="Перечень снимков")
    action.add_argument("--read", type=lambda x: int(x, 0), help="Исходный адрес, например 0x12340000")
    action.add_argument("--hex", help="Поиск байтов, например 00 01 ff 7f")
    action.add_argument("--text", help="Поиск текста в UTF-8")
    action.add_argument("--verify", action="store_true", help="Проверить сжатие и SHA-256 всех блоков выбранного снимка")
    parser.add_argument("--length", type=int, default=128, help="Число байтов для --read")
    parser.add_argument("--limit", type=int, default=100, help="Предел результатов поиска")
    parser.add_argument("--output", type=Path, help="Сохранить байты --read в новый файл")
    args = parser.parse_args()
    if args.output and args.read is None:
        parser.error("--output используется только с --read")
    try:
        archive = MemoryArchive(args.directory, args.snapshot)
        if args.list:
            for path in archive.snapshots:
                s = json.loads(path.read_text(encoding="utf-8"))
                print(json.dumps({key: s[key] for key in ["sequence", "started_at", "ended_at", "complete", "reason", "progress"]}, ensure_ascii=False))
        elif args.read is not None:
            raw = archive.read(args.read, args.length)
            if args.output:
                with args.output.open("xb") as file:
                    file.write(raw)
                print(f"Сохранено {len(raw)} байт: {args.output}")
            else:
                for offset in range(0, len(raw), 16):
                    print(f"{args.read + offset:016x}  {raw[offset:offset + 16].hex(' ')}")
        elif args.hex is not None or args.text is not None:
            needle = bytes.fromhex(args.hex) if args.hex is not None else args.text.encode("utf-8")
            for address in archive.find(needle, args.limit):
                print(f"0x{address:016x}")
        elif args.verify:
            for address in archive.addresses:
                archive.block(address)
            print(f"Проверено блоков: {len(archive.addresses)}. Снимок: {archive.sequence}. Полный: {archive.snapshot['complete']}.")
        else:
            print(json.dumps({"snapshot": archive.sequence, "complete": archive.snapshot["complete"], "reason": archive.snapshot["reason"], "blocks": len(archive.blocks), "readableBytes": sum(b["size"] for b in archive.blocks.values()), "modules": archive.snapshot["modules"]}, ensure_ascii=False, indent=2))
    except (OSError, ValueError, KeyError, TypeError, zlib.error) as error:
        print(f"Не удалось прочитать запись: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
