import { readFile } from "node:fs/promises";

import {
  NativeClip,
  NativeWriter,
  openMxfFromBuffer,
  openMxfFromPath,
  writeMxfToPath,
  type NativeClipSpec,
  type NativeReadOptions,
} from "../binding.js";

export type TrackKind = "picture" | "sound" | "data" | "other";

/**
 * An open-like source. Path strings and in-memory buffers are opened natively
 * with range-capable I/O. A custom `ByteSource` is read into memory in one
 * pass — use a path when the file is large or remote.
 */
export interface ByteSource {
  read(size?: number): Promise<Uint8Array> | Uint8Array;
  seek(offset: number, whence?: number): Promise<number> | number;
  tell(): Promise<number> | number;
  size(): Promise<number> | number;
}

export interface ReadOptions {
  readAhead?: number;
  cacheBytes?: number;
}

export const EssenceType = {
  UNKNOWN: 0,
  UNC_HD_1080P: 35,
  WAVE_PCM: 90,
  OPAQUE_PICTURE: 97,
  OPAQUE_SOUND: 98,
  OPAQUE_DATA: 99,
} as const;

export type EssenceType = (typeof EssenceType)[keyof typeof EssenceType];

export const Flavour = {
  DEFAULT: 0,
  SINGLE_PASS: 0x0008,
} as const;

export type Flavour = (typeof Flavour)[keyof typeof Flavour];

export const DescriptorKind = {
  DEFAULT: 0,
  CDCI: 1,
  RGBA: 2,
  WAVE_AUDIO: 3,
  GENERIC_DATA: 4,
} as const;

export type DescriptorKind =
  (typeof DescriptorKind)[keyof typeof DescriptorKind];

export interface PixelComponent {
  code: number;
  depth: number;
}

export interface Timecode {
  hour?: number;
  minute?: number;
  second?: number;
  frame?: number;
  dropFrame?: boolean;
}

export interface Identity {
  companyName?: string;
  productName?: string;
  versionString?: string;
  productVersion?: readonly [number, number, number, number, number];
  productUid?: Uint8Array;
  creationDate?: readonly [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
  ];
  generationUid?: Uint8Array;
  materialPackageUid?: Uint8Array;
  fileSourcePackageUid?: Uint8Array;
}

export interface TrackSpec {
  essenceType: EssenceType | number;
  samplingRate?: number;
  channelCount?: number;
  quantizationBits?: number;
  storedWidth?: number;
  storedHeight?: number;
  essenceContainerUl?: Uint8Array;
  codingUl?: Uint8Array;
  elementType?: number;
  elementLlen?: number;
  temporalReordering?: boolean;
  descriptor?: DescriptorKind | number;
  componentDepth?: number;
  horizSubsampling?: number;
  vertSubsampling?: number;
  frameLayout?: number;
  aspectRatio?: readonly [number, number];
  videoLineMap?: readonly [number, number];
  pixelLayout?: readonly PixelComponent[];
  colorPrimaries?: Uint8Array;
  transferCharacteristic?: Uint8Array;
  codingEquations?: Uint8Array;
}

export interface XmlMetadata {
  data: Uint8Array;
  schemeId?: Uint8Array;
  language?: string;
  namespace?: string;
  mimeType?: string;
  isXml?: boolean;
}

export interface ClipSpec {
  editRate: readonly [number, number];
  tracks: readonly TrackSpec[];
  flavour?: Flavour | number;
  duration?: number;
  xml?: readonly XmlMetadata[];
  startTimecode?: Timecode;
  timecodeTrack?: boolean;
  systemItem?: boolean;
  identity?: Identity;
}

export interface Track {
  index: number;
  kind: TrackKind;
  essenceType: string;
  essenceContainerUl: Uint8Array;
  codingUl?: Uint8Array;
  descriptor: DescriptorKind;
  storedWidth?: number;
  storedHeight?: number;
  displayWidth?: number;
  displayHeight?: number;
  componentDepth?: number;
  subsampling?: readonly [number, number];
  frameLayout?: number;
  aspectRatio?: readonly [number, number];
  videoLineMap?: readonly [number, number];
  pixelLayout: readonly PixelComponent[];
  colorPrimaries?: Uint8Array;
  transferCharacteristic?: Uint8Array;
  codingEquations?: Uint8Array;
  samplingRate?: number;
  channelCount?: number;
  quantizationBits?: number;
  editRate: readonly [number, number];
  duration: number;
}

export interface Frame {
  data: Uint8Array;
  elementKey: Uint8Array;
  filePosition: number;
  klSize: number;
  position: number;
  trackIndex: number;
}

export interface Package {
  frames: readonly Frame[];
}

function toNativeOptions(options: ReadOptions = {}): NativeReadOptions {
  return {
    readAhead: options.readAhead,
    cacheBytes: options.cacheBytes,
  };
}

function toNativeClipSpec(spec: ClipSpec): NativeClipSpec {
  return {
    editRateNum: spec.editRate[0],
    editRateDen: spec.editRate[1],
    flavour: spec.flavour,
    duration: spec.duration,
    tracks: spec.tracks.map((track) => ({
      essenceType: track.essenceType,
      samplingRate: track.samplingRate,
      channelCount: track.channelCount,
      quantizationBits: track.quantizationBits,
      storedWidth: track.storedWidth,
      storedHeight: track.storedHeight,
      essenceContainerUl: track.essenceContainerUl
        ? Buffer.from(track.essenceContainerUl)
        : undefined,
      codingUl: track.codingUl ? Buffer.from(track.codingUl) : undefined,
      elementType: track.elementType,
      elementLlen: track.elementLlen,
      temporalReordering: track.temporalReordering,
      descriptor: track.descriptor,
      componentDepth: track.componentDepth,
      horizSubsampling: track.horizSubsampling,
      vertSubsampling: track.vertSubsampling,
      frameLayout: track.frameLayout,
      aspectRatioNum: track.aspectRatio?.[0],
      aspectRatioDen: track.aspectRatio?.[1],
      videoLineMap: track.videoLineMap
        ? [track.videoLineMap[0], track.videoLineMap[1]]
        : undefined,
      pixelLayout: track.pixelLayout?.map((item) => ({
        code: item.code,
        depth: item.depth,
      })),
      colorPrimaries: track.colorPrimaries
        ? Buffer.from(track.colorPrimaries)
        : undefined,
      transferCharacteristic: track.transferCharacteristic
        ? Buffer.from(track.transferCharacteristic)
        : undefined,
      codingEquations: track.codingEquations
        ? Buffer.from(track.codingEquations)
        : undefined,
    })),
    xml: spec.xml?.map((item) => ({
      data: Buffer.from(item.data),
      schemeId: item.schemeId ? Buffer.from(item.schemeId) : undefined,
      language: item.language,
      namespace: item.namespace,
      mimeType: item.mimeType,
      isXml: item.isXml ?? true,
    })),
    startTimecode: spec.startTimecode
      ? {
          hour: spec.startTimecode.hour ?? 0,
          minute: spec.startTimecode.minute ?? 0,
          second: spec.startTimecode.second ?? 0,
          frame: spec.startTimecode.frame ?? 0,
          dropFrame: spec.startTimecode.dropFrame ?? false,
        }
      : undefined,
    timecodeTrack: spec.timecodeTrack,
    systemItem: spec.systemItem,
    identity: spec.identity
      ? {
          companyName: spec.identity.companyName,
          productName: spec.identity.productName,
          versionString: spec.identity.versionString,
          productVersion: spec.identity.productVersion
            ? [...spec.identity.productVersion]
            : undefined,
          productUid: spec.identity.productUid
            ? Buffer.from(spec.identity.productUid)
            : undefined,
          creationDate: spec.identity.creationDate
            ? [...spec.identity.creationDate]
            : undefined,
          generationUid: spec.identity.generationUid
            ? Buffer.from(spec.identity.generationUid)
            : undefined,
          materialPackageUid: spec.identity.materialPackageUid
            ? Buffer.from(spec.identity.materialPackageUid)
            : undefined,
          fileSourcePackageUid: spec.identity.fileSourcePackageUid
            ? Buffer.from(spec.identity.fileSourcePackageUid)
            : undefined,
        }
      : undefined,
  };
}

/**
 * An opened MXF clip. One reader per task; do not share across concurrent
 * async work.
 */
export class Clip {
  public constructor(private readonly native: NativeClip) {}

  public async info(): Promise<{
    editRate: readonly [number, number];
    duration: number;
    startTimecode?: Timecode;
    tracks: readonly Track[];
  }> {
    const info = await this.native.info();
    return {
      editRate: [info.editRateNum, info.editRateDen],
      duration: info.duration,
      startTimecode: info.startTimecode
        ? {
            hour: info.startTimecode.hour,
            minute: info.startTimecode.minute,
            second: info.startTimecode.second,
            frame: info.startTimecode.frame,
            dropFrame: info.startTimecode.dropFrame,
          }
        : undefined,
      tracks: info.tracks.map((track) => ({
        index: track.index,
        kind: track.kind as TrackKind,
        essenceType: track.essenceType,
        essenceContainerUl: Uint8Array.from(track.essenceContainerUl),
        codingUl: track.codingUl ? Uint8Array.from(track.codingUl) : undefined,
        descriptor: track.descriptor as DescriptorKind,
        storedWidth: track.storedWidth,
        storedHeight: track.storedHeight,
        displayWidth: track.displayWidth,
        displayHeight: track.displayHeight,
        componentDepth: track.componentDepth,
        subsampling:
          track.horizSubsampling !== undefined &&
          track.vertSubsampling !== undefined
            ? [track.horizSubsampling, track.vertSubsampling]
            : undefined,
        frameLayout: track.frameLayout,
        aspectRatio:
          track.aspectRatioNum !== undefined &&
          track.aspectRatioDen !== undefined
            ? [track.aspectRatioNum, track.aspectRatioDen]
            : undefined,
        videoLineMap:
          track.videoLineMap && track.videoLineMap.length >= 2
            ? [track.videoLineMap[0], track.videoLineMap[1]]
            : undefined,
        pixelLayout: (track.pixelLayout ?? []).map((item) => ({
          code: item.code,
          depth: item.depth,
        })),
        colorPrimaries: track.colorPrimaries
          ? Uint8Array.from(track.colorPrimaries)
          : undefined,
        transferCharacteristic: track.transferCharacteristic
          ? Uint8Array.from(track.transferCharacteristic)
          : undefined,
        codingEquations: track.codingEquations
          ? Uint8Array.from(track.codingEquations)
          : undefined,
        samplingRate: track.samplingRate,
        channelCount: track.channelCount,
        quantizationBits: track.quantizationBits,
        editRate: [track.editRateNum, track.editRateDen],
        duration: track.duration,
      })),
    };
  }

  public get editRate(): Promise<readonly [number, number]> {
    return this.info().then((info) => info.editRate);
  }

  public get duration(): Promise<number> {
    return this.info().then((info) => info.duration);
  }

  public get tracks(): Promise<readonly Track[]> {
    return this.info().then((info) => info.tracks);
  }

  public get startTimecode(): Promise<Timecode | undefined> {
    return this.info().then((info) => info.startTimecode);
  }

  public get xml(): Promise<readonly XmlMetadata[]> {
    return this.native.xml().then((items) =>
      items.map((item) => ({
        data: Uint8Array.from(item.data),
        schemeId: item.schemeId ? Uint8Array.from(item.schemeId) : undefined,
        language: item.language,
        namespace: item.namespace,
        mimeType: item.mimeType,
        isXml: item.isXml,
      })),
    );
  }

  public async select(tracks: Iterable<Track>): Promise<void> {
    await this.native.select(Array.from(tracks, (track) => track.index));
  }

  public async seek(position: number): Promise<void> {
    await this.native.seek(position);
  }

  public async read(count = 1): Promise<Package[]> {
    const packages = await this.native.read(count);
    return packages.map((package_) => ({
      frames: package_.frames.map((frame) => ({
        data: Uint8Array.from(frame.data),
        elementKey: Uint8Array.from(frame.elementKey),
        filePosition: frame.filePosition,
        klSize: frame.klSize,
        position: frame.position,
        trackIndex: frame.trackIndex,
      })),
    }));
  }

  public async close(): Promise<void> {
    await this.native.close();
  }
}

/**
 * An opened MXF writer. One writer per task; do not share across concurrent
 * async work. Writes go to a real path, not an in-memory buffer.
 */
export class ClipWriter {
  public constructor(private readonly native: NativeWriter) {}

  public async write(trackIndex: number, data: Uint8Array): Promise<void> {
    await this.native.write(trackIndex, Buffer.from(data));
  }

  public async finish(): Promise<void> {
    await this.native.finish();
  }

  public async close(): Promise<void> {
    await this.native.close();
  }
}

/** Open a path or buffer with native range I/O. A custom source is slurped. */
export async function openMxf(
  source: string | Uint8Array | ByteSource,
  options: ReadOptions = {},
): Promise<Clip> {
  const nativeOptions = toNativeOptions(options);
  if (typeof source === "string") {
    return new Clip(await openMxfFromPath(source, nativeOptions));
  }
  if (source instanceof Uint8Array) {
    return new Clip(
      await openMxfFromBuffer(Buffer.from(source), nativeOptions),
    );
  }
  const size = await source.size();
  const current = await source.tell();
  await source.seek(0, 0);
  const data = await source.read(size);
  await source.seek(current, 0);
  return new Clip(await openMxfFromBuffer(Buffer.from(data), nativeOptions));
}

export async function openMxfFile(
  path: string,
  options: ReadOptions = {},
): Promise<Clip> {
  return openMxf(path, options);
}

/** Write an OP1a file to a filesystem path. Pipes and custom sinks are not supported. */
export async function writeMxf(
  dest: string,
  spec: ClipSpec,
): Promise<ClipWriter> {
  return new ClipWriter(await writeMxfToPath(dest, toNativeClipSpec(spec)));
}

export async function readMxfBuffer(path: string): Promise<Uint8Array> {
  return readFile(path);
}
