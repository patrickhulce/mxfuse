"""Idiomatic Python types over the native mxfuse core."""

from __future__ import annotations

from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from enum import Enum, IntEnum
from typing import Any, Protocol, runtime_checkable

from mxfuse._mxfuse import Clip as NativeClip
from mxfuse._mxfuse import Writer as NativeWriter
from mxfuse._mxfuse import open_mxf as _open_mxf
from mxfuse._mxfuse import write_mxf as _write_mxf


@runtime_checkable
class BinarySource(Protocol):
    """Minimal open-like readable handle accepted by :func:`open_mxf`.

    ``size()`` is optional. Regular files omit it; size is inferred via
    ``seek(0, 2)``.
    """

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


class DescriptorKind(IntEnum):
    DEFAULT = 0
    CDCI = 1
    RGBA = 2
    WAVE_AUDIO = 3
    GENERIC_DATA = 4


@dataclass(frozen=True, slots=True)
class ReadOptions:
    read_ahead: int = 1 << 20
    cache_bytes: int = 64 << 20


def _as_ul(value: bytes | str | None, name: str) -> bytes | None:
    if value is None:
        return None
    raw = bytes.fromhex(value) if isinstance(value, str) else bytes(value)
    if len(raw) != 16:
        raise ValueError(f"{name} must be 16 bytes")
    return raw


@dataclass(frozen=True, slots=True)
class PixelComponent:
    code: int
    depth: int


@dataclass(frozen=True, slots=True)
class Timecode:
    hour: int = 0
    minute: int = 0
    second: int = 0
    frame: int = 0
    drop_frame: bool = False


@dataclass(frozen=True, slots=True)
class Identity:
    company_name: str | None = None
    product_name: str | None = None
    version_string: str | None = None
    product_version: tuple[int, int, int, int, int] | None = None
    product_uid: bytes | str | None = None
    creation_date: tuple[int, int, int, int, int, int, int] | None = None
    generation_uid: bytes | str | None = None
    material_package_uid: bytes | str | None = None
    file_source_package_uid: bytes | str | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "product_uid", _as_ul(self.product_uid, "product_uid"))
        object.__setattr__(
            self, "generation_uid", _as_ul(self.generation_uid, "generation_uid")
        )
        object.__setattr__(
            self,
            "material_package_uid",
            _as_umid(self.material_package_uid, "material_package_uid"),
        )
        object.__setattr__(
            self,
            "file_source_package_uid",
            _as_umid(self.file_source_package_uid, "file_source_package_uid"),
        )


def _as_umid(value: bytes | str | None, name: str) -> bytes | None:
    if value is None:
        return None
    raw = bytes.fromhex(value) if isinstance(value, str) else bytes(value)
    if len(raw) != 32:
        raise ValueError(f"{name} must be 32 bytes")
    return raw


@dataclass(frozen=True, slots=True)
class TrackSpec:
    essence_type: EssenceType
    sampling_rate: int | None = None
    channel_count: int | None = None
    quantization_bits: int | None = None
    stored_width: int | None = None
    stored_height: int | None = None
    essence_container_ul: bytes | str | None = None
    coding_ul: bytes | str | None = None
    element_type: int | None = None
    element_llen: int | None = None
    temporal_reordering: bool = False
    descriptor: DescriptorKind | None = None
    component_depth: int | None = None
    subsampling: tuple[int, int] | None = None
    frame_layout: int | None = None
    aspect_ratio: tuple[int, int] | None = None
    video_line_map: tuple[int, int] | None = None
    pixel_layout: tuple[PixelComponent, ...] | None = None
    color_primaries: bytes | str | None = None
    transfer_characteristic: bytes | str | None = None
    coding_equations: bytes | str | None = None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "essence_container_ul",
            _as_ul(self.essence_container_ul, "essence_container_ul"),
        )
        object.__setattr__(self, "coding_ul", _as_ul(self.coding_ul, "coding_ul"))
        object.__setattr__(
            self, "color_primaries", _as_ul(self.color_primaries, "color_primaries")
        )
        object.__setattr__(
            self,
            "transfer_characteristic",
            _as_ul(self.transfer_characteristic, "transfer_characteristic"),
        )
        object.__setattr__(
            self, "coding_equations", _as_ul(self.coding_equations, "coding_equations")
        )


@dataclass(frozen=True, slots=True)
class XmlMetadata:
    data: bytes
    scheme_id: bytes | str | None = None
    language: str | None = None
    namespace: str | None = None
    mime_type: str | None = None
    is_xml: bool = True

    def __post_init__(self) -> None:
        object.__setattr__(self, "scheme_id", _as_ul(self.scheme_id, "scheme_id"))


@dataclass(frozen=True, slots=True)
class ClipSpec:
    edit_rate: tuple[int, int]
    tracks: list[TrackSpec] = field(default_factory=list)
    flavour: Flavour = Flavour.DEFAULT
    duration: int | None = None
    xml: list[XmlMetadata] = field(default_factory=list)
    start_timecode: Timecode | None = None
    timecode_track: bool = True
    system_item: bool = False
    identity: Identity | None = None


@dataclass(frozen=True, slots=True)
class Track:
    index: int
    kind: TrackKind
    essence_type: str
    essence_container_ul: bytes
    coding_ul: bytes | None
    descriptor: DescriptorKind
    stored_width: int | None
    stored_height: int | None
    display_width: int | None
    display_height: int | None
    component_depth: int | None
    subsampling: tuple[int, int] | None
    frame_layout: int | None
    aspect_ratio: tuple[int, int] | None
    video_line_map: tuple[int, int] | None
    pixel_layout: tuple[PixelComponent, ...]
    color_primaries: bytes | None
    transfer_characteristic: bytes | None
    coding_equations: bytes | None
    sampling_rate: int | None
    channel_count: int | None
    quantization_bits: int | None
    edit_rate: tuple[int, int]
    duration: int


@dataclass(frozen=True, slots=True)
class Frame:
    data: bytes
    element_key: bytes
    file_position: int
    kl_size: int = 0
    position: int = 0
    track_index: int = 0


@dataclass(frozen=True, slots=True)
class Package:
    frames: tuple[Frame, ...]

    def frame(self, track_index: int) -> Frame:
        for item in self.frames:
            if item.track_index == track_index:
                return item
        raise KeyError(track_index)


def _package_from_native(package: Any) -> Package:
    return Package(
        frames=tuple(
            Frame(
                data=bytes(frame.data),
                element_key=bytes(frame.element_key),
                file_position=frame.file_position,
                kl_size=frame.kl_size,
                position=frame.position,
                track_index=frame.track_index,
            )
            for frame in package.frames
        )
    )


def _optional_bytes(value: Any) -> bytes | None:
    if value is None:
        return None
    raw = bytes(value)
    return raw or None


def _track_from_native(track: Any) -> Track:
    layout = tuple(
        PixelComponent(code=item.code, depth=item.depth) for item in track.pixel_layout
    )
    return Track(
        index=track.index,
        kind=TrackKind(track.kind),
        essence_type=track.essence_type,
        essence_container_ul=bytes(track.essence_container_ul),
        coding_ul=_optional_bytes(track.coding_ul),
        descriptor=DescriptorKind(track.descriptor),
        stored_width=track.stored_width,
        stored_height=track.stored_height,
        display_width=track.display_width,
        display_height=track.display_height,
        component_depth=track.component_depth,
        subsampling=track.subsampling,
        frame_layout=track.frame_layout,
        aspect_ratio=track.aspect_ratio,
        video_line_map=track.video_line_map,
        pixel_layout=layout,
        color_primaries=_optional_bytes(track.color_primaries),
        transfer_characteristic=_optional_bytes(track.transfer_characteristic),
        coding_equations=_optional_bytes(track.coding_equations),
        sampling_rate=track.sampling_rate,
        channel_count=track.channel_count,
        quantization_bits=track.quantization_bits,
        edit_rate=track.edit_rate,
        duration=track.duration,
    )


def _xml_from_native(item: Any) -> XmlMetadata:
    scheme = bytes(item.scheme_id)
    return XmlMetadata(
        data=bytes(item.data),
        scheme_id=scheme or None,
        language=item.language or None,
        namespace=item.namespace or None,
        mime_type=item.mime_type or None,
        is_xml=bool(item.is_xml),
    )


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
    def start_timecode(self) -> Timecode | None:
        value = self._native.start_timecode
        if value is None:
            return None
        return Timecode(
            hour=value.hour,
            minute=value.minute,
            second=value.second,
            frame=value.frame,
            drop_frame=bool(value.drop_frame),
        )

    @property
    def tracks(self) -> tuple[Track, ...]:
        return tuple(_track_from_native(track) for track in self._native.tracks)

    @property
    def xml(self) -> tuple[XmlMetadata, ...]:
        return tuple(_xml_from_native(item) for item in self._native.xml)

    def select(self, tracks: Iterable[Track]) -> None:
        self._native.select([track.index for track in tracks])

    def seek(self, position: int) -> None:
        self._native.seek(position)

    def read(self, count: int = 1) -> Iterator[Package]:
        return (_package_from_native(package) for package in self._native.read(count))

    def __iter__(self) -> Iterator[Package]:
        self.seek(0)
        for _ in range(self.duration):
            yield from self.read(count=1)

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

    def write_unit(self, *payloads: bytes) -> None:
        for index, data in enumerate(payloads):
            self.write(index, data)

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
