"""Idiomatic Python types over the native mxfuse core."""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass
from enum import Enum
from typing import Protocol, runtime_checkable

from mxfuse._mxfuse import Clip as NativeClip
from mxfuse._mxfuse import open_mxf as _open_mxf


@runtime_checkable
class BinarySource(Protocol):
    """Minimal open-like readable handle accepted by :func:`open_mxf`."""

    def read(self, size: int = -1, /) -> bytes: ...

    def seek(self, offset: int, whence: int = 0, /) -> int: ...

    def tell(self) -> int: ...


class TrackKind(str, Enum):
    PICTURE = "picture"
    SOUND = "sound"
    DATA = "data"
    OTHER = "other"


@dataclass(frozen=True, slots=True)
class ReadOptions:
    read_ahead: int = 1 << 20
    cache_bytes: int = 64 << 20


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
