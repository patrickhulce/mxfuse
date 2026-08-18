"""Idiomatic Python types over the native mxfuse core."""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from enum import Enum, IntEnum
from typing import Protocol, runtime_checkable

from mxfuse._mxfuse import Clip as NativeClip
from mxfuse._mxfuse import Writer as NativeWriter
from mxfuse._mxfuse import open_mxf as _open_mxf
from mxfuse._mxfuse import write_mxf as _write_mxf


@runtime_checkable
class BinarySource(Protocol):
    """Minimal open-like readable handle accepted by :func:`open_mxf`."""

    def read(self, size: int = -1, /) -> bytes: ...

    def seek(self, offset: int, whence: int = 0, /) -> int: ...

    def tell(self) -> int: ...


@runtime_checkable
class BinarySink(Protocol):
    """Minimal open-like writable handle accepted by :func:`write_mxf`."""

    def write(self, data: bytes, /) -> int | None: ...

    def seek(self, offset: int, whence: int = 0, /) -> int: ...

    def tell(self) -> int: ...


class TrackKind(str, Enum):
    PICTURE = "picture"
    SOUND = "sound"
    DATA = "data"
    OTHER = "other"


class EssenceType(IntEnum):
    UNKNOWN = 0
    UNC_HD_1080P = 35
    WAVE_PCM = 90
    OPAQUE_PICTURE = 97
    OPAQUE_SOUND = 98
    OPAQUE_DATA = 99


class Flavour(IntEnum):
    DEFAULT = 0
    SINGLE_PASS = 0x0008


@dataclass(frozen=True, slots=True)
class ReadOptions:
    read_ahead: int = 1 << 20
    cache_bytes: int = 64 << 20


@dataclass(frozen=True, slots=True)
class TrackSpec:
    essence_type: EssenceType
    sampling_rate: int | None = None
    channel_count: int | None = None
    quantization_bits: int | None = None
    stored_width: int | None = None
    stored_height: int | None = None
    essence_container_ul: bytes | None = None
    picture_coding_ul: bytes | None = None


@dataclass(frozen=True, slots=True)
class ClipSpec:
    edit_rate: tuple[int, int]
    tracks: list[TrackSpec] = field(default_factory=list)
    flavour: Flavour = Flavour.DEFAULT
    duration: int | None = None


@dataclass(frozen=True, slots=True)
class Track:
    index: int
    kind: TrackKind
    essence_type: str
    essence_container_ul: bytes
    edit_rate: tuple[int, int]
    duration: int


@dataclass(frozen=True, slots=True)
class Frame:
    data: bytes
    element_key: bytes
    file_position: int
    kl_size: int = 0
    position: int = 0


@dataclass(frozen=True, slots=True)
class Package:
    frames: tuple[Frame, ...]


class Clip:
    """An opened MXF clip. One reader per thread."""

    def __init__(self, native: NativeClip) -> None:
        self._native = native

    @property
    def edit_rate(self) -> tuple[int, int]:
        rate: tuple[int, int] = self._native.edit_rate
        return rate

    @property
    def duration(self) -> int:
        duration: int = self._native.duration
        return duration

    @property
    def tracks(self) -> tuple[Track, ...]:
        return tuple(
            Track(
                index=track.index,
                kind=TrackKind(track.kind),
                essence_type=track.essence_type,
                essence_container_ul=bytes(track.essence_container_ul),
                edit_rate=track.edit_rate,
                duration=track.duration,
            )
            for track in self._native.tracks
        )

    def select(self, tracks: Iterable[Track]) -> None:
        self._native.select([track.index for track in tracks])

    def seek(self, position: int) -> None:
        self._native.seek(position)

    def read(self, count: int = 1) -> Iterator[Package]:
        packages = self._native.read(count)
        return (
            Package(
                frames=tuple(
                    Frame(
                        data=bytes(frame.data),
                        element_key=bytes(frame.element_key),
                        file_position=frame.file_position,
                        kl_size=frame.kl_size,
                        position=frame.position,
                    )
                    for frame in package.frames
                )
            )
            for package in packages
        )

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> Clip:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


class Writer:
    """An opened MXF writer. One writer per thread."""

    def __init__(self, native: NativeWriter) -> None:
        self._native = native
        self._done = False

    def write(self, track_index: int, data: bytes) -> None:
        self._native.write(track_index, data)

    def finish(self) -> None:
        if self._done:
            return
        self._done = True
        self._native.finish()

    def close(self) -> None:
        if self._done:
            return
        self._done = True
        self._native.close()

    def __enter__(self) -> Writer:
        return self

    def __exit__(self, exc_type: object, *_exc: object) -> None:
        if exc_type is None:
            self.finish()
        else:
            self.close()


def open_mxf(source: BinarySource, options: ReadOptions | None = None) -> Clip:
    """Open an MXF source. The handle stays alive for the lifetime of the clip."""
    chosen = options or ReadOptions()
    return Clip(
        _open_mxf(
            source,
            read_ahead=chosen.read_ahead,
            cache_bytes=chosen.cache_bytes,
        )
    )


def write_mxf(sink: BinarySink, spec: ClipSpec) -> Writer:
    """Open an MXF sink. The handle stays alive until finish/close."""
    return Writer(_write_mxf(sink, spec))
