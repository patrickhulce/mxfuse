from io import BytesIO

import pytest

from mxfuse import Container, DecodeMode, Frame, FrameKind, Track, decode, encode


def test_container_iterates_frames() -> None:
    frames = (
        Frame(FrameKind.RAW_ESSENCE, b"one"),
        Frame(FrameKind.RAW_ESSENCE, b"two"),
    )
    container = Container(tracks=(Track(id=1, codec="jpeg2000", _frames=frames),))

    assert list(container.frames()) == list(frames)


def test_decode_is_an_explicit_scaffold_seam() -> None:
    with pytest.raises(NotImplementedError, match="Decode is not implemented"):
        decode(BytesIO(), mode=DecodeMode.PARSED)


def test_encode_is_an_explicit_scaffold_seam() -> None:
    with pytest.raises(NotImplementedError, match="Encode is not implemented"):
        encode(Container(), BytesIO())
