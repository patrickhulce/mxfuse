from pathlib import Path

import pytest

from mxfuse import ReadOptions, TrackKind, open_mxf

FIXTURE = Path(__file__).resolve().parents[3] / "tests" / "fixtures" / "sample_op1a.mxf"


@pytest.mark.skipif(not FIXTURE.is_file(), reason="sample_op1a.mxf fixture is missing")
def test_open_lists_tracks_and_reads_a_frame() -> None:
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


@pytest.mark.skipif(not FIXTURE.is_file(), reason="sample_op1a.mxf fixture is missing")
def test_disabled_tracks_yield_no_frames() -> None:
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

    def read(self, size: int = -1) -> bytes:
        self.reads += 1
        return self._file.read(size)

    def seek(self, offset: int, whence: int = 0) -> int:
        return self._file.seek(offset, whence)

    def tell(self) -> int:
        return self._file.tell()

    def close(self) -> None:
        self._file.close()


@pytest.mark.skipif(not FIXTURE.is_file(), reason="sample_op1a.mxf fixture is missing")
def test_read_ahead_amortizes_small_reads() -> None:
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
