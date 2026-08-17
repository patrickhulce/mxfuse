import { readFile } from "node:fs/promises";

import {
  NativeClip,
  openMxfFromBuffer,
  openMxfFromPath,
  type NativeReadOptions,
} from "../binding.js";

export type TrackKind = "picture" | "sound" | "data" | "other";

/** An open-like source. Path strings and in-memory buffers are opened natively. */
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

export interface Track {
  index: number;
  kind: TrackKind;
  essenceType: string;
  essenceContainerUl: Uint8Array;
  editRate: readonly [number, number];
  duration: number;
}

export interface Frame {
  data: Uint8Array;
  elementKey: Uint8Array;
  filePosition: number;
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

/**
 * An opened MXF clip. One reader per task; do not share across concurrent
 * async work.
 */
export class Clip {
  public constructor(private readonly native: NativeClip) {}

  public async info(): Promise<{
    editRate: readonly [number, number];
    duration: number;
    tracks: readonly Track[];
  }> {
    const info = await this.native.info();
    return {
      editRate: [info.editRateNum, info.editRateDen],
      duration: info.duration,
      tracks: info.tracks.map((track) => ({
        index: track.index,
        kind: track.kind as TrackKind,
        essenceType: track.essenceType,
        essenceContainerUl: Uint8Array.from(track.essenceContainerUl),
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
      })),
    }));
  }

  public async close(): Promise<void> {
    await this.native.close();
  }
}

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

export async function readMxfBuffer(path: string): Promise<Uint8Array> {
  return readFile(path);
}
