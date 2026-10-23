"""Public Python API for mxfuse."""

from importlib.metadata import PackageNotFoundError, version

from mxfuse._core import (
    BinarySink,
    BinarySource,
    Clip,
    ClipSpec,
    DescriptorKind,
    EssenceType,
    Flavour,
    Frame,
    Identity,
    Package,
    PixelComponent,
    ReadOptions,
    TextEncoding,
    Timecode,
    Track,
    TrackKind,
    TrackSpec,
    Writer,
    XmlMetadata,
    open_mxf,
    write_mxf,
)

try:
    __version__ = version("mxfuse")
except PackageNotFoundError:
    __version__ = "0.0.0-dev"

__all__ = [
    "__version__",
    "BinarySink",
    "BinarySource",
    "Clip",
    "ClipSpec",
    "DescriptorKind",
    "EssenceType",
    "Flavour",
    "Frame",
    "Identity",
    "Package",
    "PixelComponent",
    "ReadOptions",
    "Timecode",
    "Track",
    "TrackKind",
    "TrackSpec",
    "TextEncoding",
    "Writer",
    "XmlMetadata",
    "open_mxf",
    "write_mxf",
]
