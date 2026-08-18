#!/usr/bin/env python3
"""Emit a normalized MXF structural profile as JSON.

Identity-variable fields (UMIDs, timestamps, GenerationUID, Identification,
byte offsets, payload sizes) are stripped so two writers can be compared.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

# SMPTE set / pack keys (16-byte hex).
HEADER_PREFIX = "060e2b34020501010d0102010102"
BODY_PREFIX = "060e2b34020501010d0102010103"
FOOTER_PREFIX = "060e2b34020501010d0102010104"
PRIMER_KEY = "060e2b34020501010d01020101050100"
RIP_KEY = "060e2b34020501010d01020101110100"
INDEX_SEGMENT_KEY = "060e2b34025301010d01020101100100"
IDENTIFICATION_KEY = "060e2b34025301010d01010101013000"
RGBA_KEY = "060e2b34025301010d01010101012900"
CDCI_KEY = "060e2b34025301010d01010101012800"
GENERIC_DATA_KEY = "060e2b34025301010d01010101014300"
WAVE_KEY = "060e2b34025301010d01010101014800"
SYSTEM_ITEM_KEY = "060e2b34020501010d01030104010100"

# File Descriptor / picture items that are structural, not identity.
SKIP_ITEM_ULS = {
    "060e2b34010101010101150200000000",  # InstanceUID
    "060e2b340101010201010d0000000000",  # GenerationUID
    "060e2b34010101010207020110020000",  # LinkedGenerationUID
    "060e2b34010101050601010305000000",  # LinkedTrackID
    "060e2b34010101010406010200000000",  # ContainerDuration
    # Optional zero geometry offsets; some writers omit them.
    "060e2b34010101010401050109000000",  # SampledXOffset
    "060e2b3401010101040105010a000000",  # SampledYOffset
    "060e2b3401010101040105010d000000",  # DisplayXOffset
    "060e2b3401010101040105010e000000",  # DisplayYOffset
}

# Index Table Segment: DeltaEntryArray (static local tag 0x3f09; also the UL).
DELTA_ENTRY_ARRAY_UL = "060e2b34010101050406020100000000"
DELTA_ENTRY_ARRAY_TAG = 0x3F09

DESCRIPTOR_KEYS = {
    RGBA_KEY: "RGBAEssenceDescriptor",
    CDCI_KEY: "CDCIEssenceDescriptor",
    GENERIC_DATA_KEY: "GenericDataEssenceDescriptor",
    WAVE_KEY: "WaveAudioDescriptor",
}


def hx(data: bytes) -> str:
    return data.hex()


def read_ber(data: bytes, offset: int) -> tuple[int, int]:
    first = data[offset]
    if first < 0x80:
        return first, 1
    width = first & 0x7F
    return int.from_bytes(data[offset + 1 : offset + 1 + width], "big"), 1 + width


def iter_klv(data: bytes) -> list[tuple[int, bytes, int, int, bytes]]:
    items: list[tuple[int, bytes, int, int, bytes]] = []
    offset = 0
    while offset + 17 <= len(data):
        key = data[offset : offset + 16]
        if key[:4] != bytes.fromhex("060e2b34"):
            break
        length, llen = read_ber(data, offset + 16)
        start = offset + 16 + llen
        value = data[start : start + length]
        items.append((offset, key, llen, length, value))
        offset = start + length
    return items


def parse_local_set(body: bytes) -> list[tuple[int, bytes]]:
    items: list[tuple[int, bytes]] = []
    pos = 0
    while pos + 4 <= len(body):
        tag = int.from_bytes(body[pos : pos + 2], "big")
        size = int.from_bytes(body[pos + 2 : pos + 4], "big")
        items.append((tag, body[pos + 4 : pos + 4 + size]))
        pos += 4 + size
    return items


def parse_primer(value: bytes) -> dict[int, str]:
    if len(value) < 8:
        return {}
    count = int.from_bytes(value[0:4], "big")
    size = int.from_bytes(value[4:8], "big")
    mapping: dict[int, str] = {}
    for index in range(count):
        entry = value[8 + index * size : 8 + (index + 1) * size]
        if len(entry) < 18:
            continue
        mapping[int.from_bytes(entry[0:2], "big")] = hx(entry[2:18])
    return mapping


def partition_kind(key: bytes) -> str | None:
    hex_key = hx(key)
    if hex_key.startswith(HEADER_PREFIX):
        return "Header"
    if hex_key.startswith(BODY_PREFIX):
        return "Body"
    if hex_key.startswith(FOOTER_PREFIX):
        return "Footer"
    return None


def parse_partition_pack(value: bytes) -> dict[str, Any]:
    # Partition Pack value (ST 377-1 / libMXF):
    # major(2) minor(2) kag(4) this(8) prev(8) footer(8)
    # header_byte_count(8) index_byte_count(8) index_sid(4)
    # body_offset(8) body_sid(4) operational_pattern(16)
    # essence_containers batch: count(4) size(4) then ULs. Fixed prefix is 88 bytes.
    if len(value) < 88:
        return {}
    index_sid = int.from_bytes(value[48:52], "big")
    body_sid = int.from_bytes(value[60:64], "big")
    op_pattern = hx(value[64:80])
    containers: list[str] = []
    count = int.from_bytes(value[80:84], "big")
    size = int.from_bytes(value[84:88], "big")
    if size == 16 and 0 < count <= 64 and 88 + count * size <= len(value):
        for index in range(count):
            start = 88 + index * size
            containers.append(hx(value[start : start + 16]))
        containers.sort()
    return {
        "body_sid": body_sid,
        "index_sid": index_sid,
        "operational_pattern": op_pattern,
        "essence_containers": containers,
    }


def parse_delta_entries(value: bytes) -> list[dict[str, int]]:
    if len(value) < 8:
        return []
    count = int.from_bytes(value[0:4], "big")
    size = int.from_bytes(value[4:8], "big")
    entries: list[dict[str, int]] = []
    for index in range(count):
        raw = value[8 + index * size : 8 + (index + 1) * size]
        if len(raw) < 2:
            continue
        pos_table = int.from_bytes(raw[0:1], "big", signed=True)
        slice_index = raw[1]
        entries.append({"pos_table_index": pos_table, "slice": slice_index})
    return entries


def profile_bytes(data: bytes) -> dict[str, Any]:
    klvs = iter_klv(data)
    primer: dict[int, str] = {}
    partitions: list[dict[str, Any]] = []
    descriptors: list[dict[str, Any]] = []
    essence_elements: list[dict[str, Any]] = []
    index_deltas: list[list[dict[str, int]]] = []
    content_keys: list[str] = []
    seen_essence: set[str] = set()
    has_system_item = False
    has_rip = False

    for _offset, key, llen, _length, value in klvs:
        hex_key = hx(key)
        kind = partition_kind(key)
        if kind:
            pack = parse_partition_pack(value)
            partitions.append({"kind": kind, **pack})
            continue
        if hex_key == PRIMER_KEY:
            primer = parse_primer(value)
            continue
        if hex_key == RIP_KEY:
            has_rip = True
            continue
        if hex_key == IDENTIFICATION_KEY:
            continue
        if hex_key == SYSTEM_ITEM_KEY:
            has_system_item = True
            continue
        if hex_key == INDEX_SEGMENT_KEY:
            for tag, item in parse_local_set(value):
                ul = primer.get(tag, "")
                if tag == DELTA_ENTRY_ARRAY_TAG or ul == DELTA_ENTRY_ARRAY_UL:
                    index_deltas.append(parse_delta_entries(item))
            continue
        if hex_key in DESCRIPTOR_KEYS:
            items = []
            for tag, item in parse_local_set(value):
                ul = primer.get(tag, f"tag:{tag:04x}")
                if ul in SKIP_ITEM_ULS:
                    continue
                if len(item) > 256:
                    continue
                items.append({"ul": ul, "value": hx(item)})
            items.sort(key=lambda entry: entry["ul"])
            descriptors.append({"kind": DESCRIPTOR_KEYS[hex_key], "items": items})
            continue
        if key[12] in (0x15, 0x16, 0x17) and hex_key.startswith("060e2b34010201010d010301"):
            if hex_key not in seen_essence:
                seen_essence.add(hex_key)
                essence_elements.append(
                    {
                        "key": hex_key,
                        "item_type": key[12],
                        "element_type": key[14],
                        "llen": llen,
                    }
                )
            content_keys.append(hex_key)

    essence_elements.sort(key=lambda entry: entry["key"])
    content_pattern = _collapse_pattern(content_keys)
    return {
        "partitions": partitions,
        "essence_elements": essence_elements,
        "content_package": content_pattern,
        "descriptors": descriptors,
        "index_delta_entries": index_deltas[:1],
        "has_system_item": has_system_item,
        "has_rip": has_rip,
    }


def _collapse_pattern(keys: list[str]) -> list[str]:
    if not keys:
        return []
    unique = list(dict.fromkeys(keys))
    if len(keys) >= 2 * len(unique):
        return unique
    return unique


def profile_mxf(path: Path) -> dict[str, Any]:
    return profile_bytes(path.read_bytes())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mxf", type=Path)
    parser.add_argument("-o", "--output", type=Path)
    args = parser.parse_args()
    profile = profile_mxf(args.mxf)
    text = json.dumps(profile, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
