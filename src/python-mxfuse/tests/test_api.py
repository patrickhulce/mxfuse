from io import BytesIO
from pathlib import Path

import pytest

from mxfuse import (
    ClipSpec,
    EssenceType,
    Flavour,
    ReadOptions,
    TextEncoding,
    TrackKind,
    TrackSpec,
    XmlMetadata,
    open_mxf,
    write_mxf,
)

FIXTURE = Path(__file__).resolve().parents[3] / "tests" / "fixtures" / "sample_op1a.mxf"


def require_fixture() -> None:
    assert FIXTURE.is_file(), f"missing {FIXTURE}; run ./scripts/generate-fixture.sh"


def test_open_lists_tracks_and_reads_a_frame() -> None:
    require_fixture()
    with FIXTURE.open("rb") as handle, open_mxf(handle) as clip:
        assert clip.duration > 0
        assert clip.edit_rate[1] != 0
        assert clip.tracks
        clip.select(track for track in clip.tracks if track.kind is TrackKind.PICTURE)
        clip.seek(0)
        packages = list(clip.read(count=1))
        assert packages
        assert packages[0].frames
        frame = packages[0].frames[0]
        assert frame.data
        assert len(frame.element_key) == 16


def test_disabled_tracks_yield_no_frames() -> None:
    require_fixture()
    with FIXTURE.open("rb") as handle, open_mxf(handle) as clip:
        picture = [track for track in clip.tracks if track.kind is TrackKind.PICTURE]
        if not picture:
            pytest.skip("fixture has no picture track")
        clip.select(picture)
        clip.seek(0)
        packages = list(clip.read(count=1))
        kinds = {track.kind for track in clip.tracks}
        if TrackKind.SOUND in kinds:
            assert all(len(package.frames) == 1 for package in packages)


class CountingSource:
    def __init__(self, path: Path) -> None:
        self._file = path.open("rb")
        self.reads = 0
        self.bytes = 0

    def read(self, size: int = -1) -> bytes:
        self.reads += 1
        data = self._file.read(size)
        self.bytes += len(data)
        return data

    def seek(self, offset: int, whence: int = 0) -> int:
        return self._file.seek(offset, whence)

    def tell(self) -> int:
        return self._file.tell()

    def close(self) -> None:
        self._file.close()


def test_read_ahead_amortizes_small_reads() -> None:
    require_fixture()
    bare = CountingSource(FIXTURE)
    cached = CountingSource(FIXTURE)
    try:
        with open_mxf(bare, ReadOptions(read_ahead=0, cache_bytes=0)) as clip:
            clip.seek(min(2, max(clip.duration - 1, 0)))
            list(clip.read(count=1))
        with open_mxf(
            cached, ReadOptions(read_ahead=1 << 20, cache_bytes=8 << 20)
        ) as clip:
            clip.seek(min(2, max(clip.duration - 1, 0)))
            list(clip.read(count=1))
        assert cached.reads < bare.reads
    finally:
        bare.close()
        cached.close()


def test_one_frame_byte_budget() -> None:
    require_fixture()
    source = CountingSource(FIXTURE)
    try:
        with open_mxf(source, ReadOptions(read_ahead=0, cache_bytes=0)) as clip:
            clip.select(
                track for track in clip.tracks if track.kind is TrackKind.PICTURE
            )
            clip.seek(0)
            packages = list(clip.read(count=1))
            assert packages
            assert packages[0].frames
        fixture_size = FIXTURE.stat().st_size
        assert source.bytes < fixture_size // 2, (
            f"one-frame fetch ({source.bytes} bytes) was not well under "
            f"fixture size ({fixture_size} bytes)"
        )
    finally:
        source.close()


def test_write_round_trip_unc_and_pcm() -> None:
    picture = bytes([0x11]) * (1920 * 1080 * 2)
    audio = bytes([0x33]) * (1920 * 2)
    sink = BytesIO()
    spec = ClipSpec(
        edit_rate=(25, 1),
        flavour=Flavour.DEFAULT,
        duration=1,
        tracks=[
            TrackSpec(EssenceType.UNC_HD_1080P),
            TrackSpec(
                EssenceType.WAVE_PCM,
                sampling_rate=48000,
                channel_count=1,
                quantization_bits=16,
            ),
        ],
    )
    with write_mxf(sink, spec) as clip:
        clip.write(0, picture)
        clip.write(1, audio)
    sink.seek(0)
    with open_mxf(sink) as clip:
        assert clip.duration == 1
        assert len(clip.tracks) == 2
        clip.select(clip.tracks)
        clip.seek(0)
        packages = list(clip.read(count=1))
        assert packages[0].frames[0].data == picture
        assert packages[0].frames[1].data == audio
        assert packages[0].frames[0].kl_size > 0
        assert packages[0].frames[0].track_index == 0
        assert packages[0].frames[1].track_index == 1
        assert packages[0].frame(0).data == picture
        assert packages[0].frame(1).data == audio


def test_clip_name_round_trip() -> None:
    def written(clip_name: str | None) -> bytes:
        sink = BytesIO()
        spec = ClipSpec(
            edit_rate=(25, 1),
            flavour=Flavour.DEFAULT,
            duration=1,
            tracks=[TrackSpec(EssenceType.UNC_HD_1080P)],
            clip_name=clip_name,
        )
        with write_mxf(sink, spec) as clip:
            clip.write(0, bytes([0x11]) * (1920 * 1080 * 2))
        return sink.getvalue()

    with open_mxf(BytesIO(written("shot_001 fixed_p0_rgc_grain"))) as clip:
        assert clip.name == "shot_001 fixed_p0_rgc_grain"
    with open_mxf(BytesIO(written(None))) as clip:
        assert clip.name is None


def test_write_round_trip_xml_and_opaque() -> None:
    container = bytes.fromhex("060e2b34040101010d010301027f0101")
    coding = "060e2b3404010101040102017f000000"
    xml = b'<clip xmlns="urn:x-mxfuse:test">hello</clip>'
    sink = BytesIO()
    spec = ClipSpec(
        edit_rate=(24, 1),
        duration=2,
        tracks=[
            TrackSpec(
                EssenceType.OPAQUE_PICTURE,
                stored_width=64,
                stored_height=32,
                essence_container_ul=container,
                coding_ul=coding,
            )
        ],
        xml=[XmlMetadata(data=xml, language="en", namespace="urn:x-mxfuse:test")],
    )
    with write_mxf(sink, spec) as writer:
        writer.write_unit(b"frame-a")
        writer.write_unit(b"frame-b")
    sink.seek(0)
    with open_mxf(sink) as clip:
        assert clip.xml[0].data == xml
        assert clip.xml[0].is_xml
        assert clip.xml[0].text_encoding == TextEncoding.UTF8
        clip.select(clip.tracks)
        packages = list(clip)
        recovered = [package.frame(0).data for package in packages]
        assert recovered == [b"frame-a", b"frame-b"]
        clip.seek(0)
        by_count = list(clip.read(count=2))
        recovered = [package.frame(0).data for package in by_count]
        assert recovered == [b"frame-a", b"frame-b"]


def _opaque_spec(xml: XmlMetadata) -> ClipSpec:
    return ClipSpec(
        edit_rate=(24, 1),
        duration=1,
        tracks=[
            TrackSpec(
                EssenceType.OPAQUE_PICTURE,
                stored_width=64,
                stored_height=32,
                essence_container_ul=bytes.fromhex("060e2b34040101010d010301027f0101"),
            )
        ],
        xml=[xml],
    )


def test_xml_scheme_match_and_validation() -> None:
    urn = "urn:x-jxlmxf:exr-header-metadata:1"
    other = "urn:x-jxlmxf:singularity-metadata:1"
    payload = b'<clip xmlns="urn:x-mxfuse:test">hello</clip>'

    def write(item: XmlMetadata) -> bytes:
        sink = BytesIO()
        with write_mxf(sink, _opaque_spec(item)) as writer:
            writer.write_unit(b"frame")
        return sink.getvalue()

    first = write(
        XmlMetadata(data=payload, scheme_id=urn, namespace="urn:x-mxfuse:test")
    )
    second = write(
        XmlMetadata(data=payload, scheme_id=urn, namespace="urn:x-mxfuse:test")
    )
    other_bytes = write(
        XmlMetadata(data=payload, scheme_id=other, namespace="urn:x-mxfuse:test")
    )
    defaulted = write(XmlMetadata(data=payload, namespace="urn:x-mxfuse:test"))

    with open_mxf(BytesIO(first)) as clip:
        scheme = clip.xml[0].scheme_id
        assert isinstance(scheme, bytes)
        assert len(scheme) == 16
        assert clip.xml[0].data == payload
        assert clip.xml[0].matches_scheme(urn)
        assert not clip.xml[0].matches_scheme(other)
        assert XmlMetadata.is_scheme_match(urn, scheme)
        assert XmlMetadata(data=payload, scheme_id=urn).matches_scheme(urn)

    with open_mxf(BytesIO(second)) as clip:
        assert clip.xml[0].scheme_id == scheme
    with open_mxf(BytesIO(other_bytes)) as clip:
        assert clip.xml[0].scheme_id != scheme
    with open_mxf(BytesIO(defaulted)) as clip:
        assert clip.xml[0].scheme_id != scheme

    raw_ul = bytes(range(16))
    ul_file = write(
        XmlMetadata(data=payload, scheme_id=raw_ul, namespace="urn:x-mxfuse:test")
    )
    with open_mxf(BytesIO(ul_file)) as clip:
        assert clip.xml[0].scheme_id == raw_ul

    with pytest.raises(Exception, match="malformed xml") as malformed:
        write(
            XmlMetadata(
                data=b"<clip>",
                scheme_id=urn,
                namespace="urn:x-mxfuse:test",
            )
        )
    assert urn in str(malformed.value)
    assert "urn:x-mxfuse:test" in str(malformed.value)

    with pytest.raises(Exception, match="BOM") as bom:
        write(
            XmlMetadata(
                data=b"\xef\xbb\xbf" + payload,
                scheme_id=urn,
                namespace="urn:x-mxfuse:test",
            )
        )
    assert urn in str(bom.value)
