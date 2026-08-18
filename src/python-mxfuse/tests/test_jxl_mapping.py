from __future__ import annotations

import json
import sys
from io import BytesIO
from pathlib import Path

from mxfuse import (
    ClipSpec,
    DescriptorKind,
    EssenceType,
    Flavour,
    PixelComponent,
    TrackSpec,
    XmlMetadata,
    write_mxf,
)

ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / "scripts"
GOLDEN = ROOT / "tests" / "fixtures" / "jxl_mapping.profile.json"

sys.path.insert(0, str(SCRIPTS))
from mxf_structure import profile_bytes  # noqa: E402

JXL_CONTAINER = bytes.fromhex("060e2b340401010d0d01030102700100")
JXL_CODING = bytes.fromhex("060e2b340401010d0401020270000000")
JXL_META_CONTAINER = bytes.fromhex("060e2b340401010d0d01030102700300")
XML = (
    b'<?xml version="1.0" encoding="UTF-8"?>'
    b'<jxlmxf-exr-meta xmlns="http://jxlmxf/exr-meta/1" version="1"/>'
)


def test_jxl_mapping_matches_reference_profile() -> None:
    sink = BytesIO()
    spec = ClipSpec(
        edit_rate=(24, 1),
        flavour=Flavour.DEFAULT,
        duration=2,
        system_item=True,
        tracks=[
            TrackSpec(
                EssenceType.OPAQUE_PICTURE,
                stored_width=96,
                stored_height=64,
                essence_container_ul=JXL_CONTAINER,
                coding_ul=JXL_CODING,
                element_type=0x70,
                element_llen=8,
                temporal_reordering=True,
                descriptor=DescriptorKind.RGBA,
                frame_layout=0,
                aspect_ratio=(16, 9),
                video_line_map=(1, 0),
                pixel_layout=(
                    PixelComponent(code=ord("R"), depth=32),
                    PixelComponent(code=ord("G"), depth=32),
                    PixelComponent(code=ord("B"), depth=32),
                ),
            ),
            TrackSpec(
                EssenceType.OPAQUE_DATA,
                essence_container_ul=JXL_META_CONTAINER,
                element_type=0x70,
                descriptor=DescriptorKind.GENERIC_DATA,
            ),
        ],
        xml=[
            XmlMetadata(
                data=XML,
                language="en",
                namespace="http://jxlmxf/exr-meta/1",
            )
        ],
    )
    with write_mxf(sink, spec) as writer:
        writer.write_unit(b"jxl-frame-a", b"meta-a")
        writer.write_unit(b"jxl-frame-b", b"meta-b")

    actual = profile_bytes(sink.getvalue())
    expected = json.loads(GOLDEN.read_text())
    assert actual == expected
