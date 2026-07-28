"""Idiomatic Python types over the native mxfuse core."""

from __future__ import annotations

from collections.abc import Iterator, Mapping
from dataclasses import dataclass, field
from enum import Enum
from typing import Protocol, runtime_checkable

from mxfuse._mxfuse import decode_scaffold, encode_scaffold


class DecodeMode(str, Enum):
    """Controls whether frames contain raw essence or decoded pixels."""

    RAW = "raw"
    PARSED = "parsed"


@runtime_checkable
class BinarySource(Protocol):
    """Minimal open-like readable handle accepted by :func:`decode`."""

    def read(self, size: int = -1, /) -> bytes: ...

    def seek(self, offset: int, whence: int = 0, /) -> int: ...

    def tell(self) -> int: ...


@runtime_checkable
class BinarySink(Protocol):
    """Minimal open-like writable handle accepted by :func:`encode`."""

    def write(self, data: bytes, /) -> int: ...

    def seek(self, offset: int, whence: int = 0, /) -> int: ...

    def tell(self) -> int: ...


@dataclass(frozen=True, slots=True)
class Metadata:
    """Extensible string metadata attached to a container or track."""

    values: Mapping[str, str] = field(default_factory=dict)


class FrameKind(str, Enum):
    RAW_ESSENCE = "raw_essence"
    PIXELS = "pixels"


@dataclass(frozen=True, slots=True)
class Frame:
    kind: FrameKind
    data: bytes


@dataclass(frozen=True, slots=True)
class Track:
    id: int
    codec: str | None = None
    metadata: Metadata = field(default_factory=Metadata)
    _frames: tuple[Frame, ...] = ()

    def frames(self) -> Iterator[Frame]:
        """Yield frames without constructing an additional collection."""
        yield from self._frames


@dataclass(frozen=True, slots=True)
class Container:
    tracks: tuple[Track, ...] = ()
    metadata: Metadata = field(default_factory=Metadata)
    mode: DecodeMode = DecodeMode.RAW

    def frames(self) -> Iterator[Frame]:
        """Yield the frames from each track in track order."""
        for track in self.tracks:
            yield from track.frames()


def decode(source: BinarySource, *, mode: DecodeMode = DecodeMode.RAW) -> Container:
    """Decode an open-like source.

    Parsing is the next implementation milestone; this scaffold establishes
    the handle-based API and raises ``NotImplementedError`` from the Rust core.
    """
    del source
    decode_scaffold(mode.value)
    raise AssertionError("native decoder unexpectedly returned")


def encode(container: Container, destination: BinarySink) -> None:
    """Encode ``container`` to an open-like destination."""
    del container, destination
    encode_scaffold()
