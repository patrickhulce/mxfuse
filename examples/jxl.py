#!/usr/bin/env python3
"""Round-trip JPEG XL through the experimental jpeg_xl_mxf Generic Container mapping.

mxfuse is a container library: essence goes in as bytes and comes back out.
The picture track is OPAQUE_PICTURE with the mapping's container UL, coding UL,
element type 0x70, 8-byte BER length, and an RGBA descriptor. Clip-level XML
uses the mapping namespace; per-frame digests ride on an OPAQUE_DATA track.

    make examples
    uv run --project src/python-mxfuse --with imagecodecs python examples/jxl.py
"""

from hashlib import sha256
from pathlib import Path
from subprocess import run
from tempfile import TemporaryDirectory

import numpy as np
from imagecodecs import jpegxl_decode, jpegxl_encode

from mxfuse import (
    ClipSpec,
    DescriptorKind,
    EssenceType,
    PixelComponent,
    TrackSpec,
    XmlMetadata,
    open_mxf,
    write_mxf,
)

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / ".data/4KProRes.mov"
OUT = ROOT / ".data/jxl-demo"

# 4KProRes.mov, per ffprobe: 3840x2160, 24000/1001, 48 kHz / 24-bit / stereo.
# PCM per edit unit is sample_rate * den / num: 48000 * 1001 / 24000 = 2002.
WIDTH, HEIGHT, FRAMES = 3840, 2160, 20
EDIT_RATE = (24000, 1001)
CHANNELS, BITS = 2, 24
PCM_PER_FRAME = 2002 * CHANNELS * (BITS // 8)

JXL_CONTAINER = "060e2b340401010d0d01030102700100"
JXL_CODING = "060e2b340401010d0401020270000000"
JXL_META_CONTAINER = "060e2b340401010d0d01030102700300"
XML_NS = "http://jxlmxf/exr-meta/1"

CLIP_XML = (
    f'<clip xmlns="{XML_NS}" codec="JPEG XL" width="{WIDTH}" height="{HEIGHT}" '
    f'editRate="{EDIT_RATE[0]}/{EDIT_RATE[1]}" sampleRate="48000" '
    f'channels="{CHANNELS}" bits="{BITS}"/>'
).encode()

SPEC = ClipSpec(
    edit_rate=EDIT_RATE,
    duration=FRAMES,
    system_item=True,
    tracks=[
        TrackSpec(
            EssenceType.OPAQUE_PICTURE,
            stored_width=WIDTH,
            stored_height=HEIGHT,
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
            EssenceType.WAVE_PCM,
            sampling_rate=48000,
            channel_count=CHANNELS,
            quantization_bits=BITS,
        ),
        TrackSpec(
            EssenceType.OPAQUE_DATA,
            essence_container_ul=JXL_META_CONTAINER,
            element_type=0x70,
            descriptor=DescriptorKind.GENERIC_DATA,
        ),
    ],
    xml=[XmlMetadata(data=CLIP_XML, language="en", namespace=XML_NS)],
)


def sh(command: str) -> None:
    run(command, shell=True, check=True)


def encode(workdir: Path) -> tuple[list[bytes], bytes]:
    raw = workdir / "frames.rgb"
    sh(
        f'ffmpeg -v error -i "{SOURCE}" -frames:v {FRAMES} '
        f'-pix_fmt rgb48le -f rawvideo "{raw}"'
    )
    pixels = np.fromfile(raw, dtype="<u2").reshape(FRAMES, HEIGHT, WIDTH, 3)
    pixels = pixels.astype(np.float32) / 65535.0
    raw.unlink()
    frames = []
    for index, frame in enumerate(pixels):
        data = jpegxl_encode(frame, effort=7, distance=1.0, lossless=False)
        frames.append(data)
        print(f"  frame {index:>3}  {len(data):>9,} bytes")
    raw = workdir / "audio.raw"
    sh(
        f'ffmpeg -v error -i "{SOURCE}" -map 0:a:0 -ac {CHANNELS} -ar 48000 '
        f'-c:a pcm_s24le -f s24le "{raw}"'
    )
    needed = FRAMES * PCM_PER_FRAME
    pcm = raw.read_bytes()[:needed].ljust(needed, b"\x00")
    raw.unlink()
    return frames, pcm


def frame_xml(index: int, jxl: bytes) -> bytes:
    return (
        f'<frame xmlns="{XML_NS}" index="{index}" '
        f'timecode="00:00:{index // 24:02}:{index % 24:02}" '
        f'sha256="{sha256(jxl).hexdigest()}"/>'
    ).encode()


def decode(workdir: Path, frames: list[bytes], pcm: bytes) -> None:
    raw = workdir / "out.rgb"
    with raw.open("wb") as handle:
        for data in frames:
            rgb = jpegxl_decode(data)
            rgb16 = np.clip(np.rint(rgb * 65535.0), 0, 65535).astype("<u2")
            handle.write(rgb16.tobytes())
    (workdir / "out.raw").write_bytes(pcm)
    num, den = EDIT_RATE
    size = f"{WIDTH}x{HEIGHT}"
    sh(
        f"ffmpeg -v error -y -framerate {num}/{den} -f rawvideo "
        f'-pix_fmt rgb48le -s {size} -i "{raw}" '
        f'-f s24le -ar 48000 -ac {CHANNELS} -i "{workdir}/out.raw" '
        f"-c:v prores_ks -profile:v 3 -pix_fmt yuv422p10le -c:a pcm_s24le "
        f'"{OUT}/roundtrip.mov"'
    )
    sh(
        f"ffmpeg -v error -y -f rawvideo -pix_fmt rgb48le -s {size} "
        f'-i "{raw}" -frames:v 1 -pix_fmt rgb24 "{OUT}/frame00000.png"'
    )


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    print(
        f"source   {SOURCE.name}: {WIDTH}x{HEIGHT}, "
        f"{EDIT_RATE[0]}/{EDIT_RATE[1]}, {FRAMES} frames"
    )
    with TemporaryDirectory(prefix="mxfuse-jxl-") as tmp:
        workdir = Path(tmp)
        print("encode   JPEG XL float32 at effort 7, distance 1.0")
        frames, pcm = encode(workdir)

        target = OUT / "jxl.mxf"
        with target.open("wb") as handle, write_mxf(handle, SPEC) as writer:
            for index, jxl in enumerate(frames):
                start = index * PCM_PER_FRAME
                writer.write_unit(
                    jxl, pcm[start : start + PCM_PER_FRAME], frame_xml(index, jxl)
                )
        print(f"mux      {target} ({target.stat().st_size:,} bytes)")

        picture: list[bytes] = []
        sound = bytearray()
        meta: list[bytes] = []
        with target.open("rb") as handle, open_mxf(handle) as clip:
            print(
                f"demux    edit rate {clip.edit_rate[0]}/{clip.edit_rate[1]}, "
                f"{clip.duration} edit units"
            )
            for track in clip.tracks:
                coding = track.coding_ul.hex() if track.coding_ul else "-"
                print(
                    f"  track {track.index}  {track.kind.value:<7} "
                    f"{track.essence_container_ul.hex()}  coding={coding}"
                )
                if track.coding_ul == bytes.fromhex(JXL_CODING):
                    layout = "".join(chr(item.code) for item in track.pixel_layout)
                    size = f"{track.stored_width}x{track.stored_height}"
                    print(
                        f"           JPEG XL  {size} {layout} "
                        f"descriptor={track.descriptor.name}"
                    )
            manifest = clip.xml[0].data
            for package in clip:
                picture.append(package.frame(0).data)
                sound += package.frame(1).data
                meta.append(package.frame(2).data)

        expected_meta = [frame_xml(i, jxl) for i, jxl in enumerate(frames)]
        if picture != frames or bytes(sound) != pcm or meta != expected_meta:
            raise SystemExit("essence did not survive the round trip")
        for jxl, document in zip(picture, meta, strict=True):
            if sha256(jxl).hexdigest() not in document.decode():
                raise SystemExit("frame digest does not match XML")
        print("verify   picture, sound, data identical; digests match XML")

        print(f"decode   {len(picture)} JPEG XL frames back to ProRes")
        decode(workdir, picture, bytes(sound))

    print(f"\nwrote {target}")
    print(f"wrote {OUT / 'roundtrip.mov'}")
    print(f"wrote {OUT / 'frame00000.png'}")
    print("\nclip XML:")
    print(manifest.decode())
    print("first frame XML:")
    print(meta[0].decode())


if __name__ == "__main__":
    main()
