import {
  decodeScaffold as decodeNative,
  encodeScaffold as encodeNative,
} from "../binding.js";

export type DecodeMode = "raw" | "parsed";
export type FrameKind = "raw_essence" | "pixels";

/** An open-like source, including adapters backed by remote object stores. */
export interface BinarySource {
  read(size?: number): Uint8Array | Promise<Uint8Array>;
  seek?(offset: number, whence?: number): number | Promise<number>;
  tell?(): number | Promise<number>;
}

/** An open-like destination, including adapters backed by remote object stores. */
export interface BinarySink {
  write(data: Uint8Array): number | Promise<number>;
  seek?(offset: number, whence?: number): number | Promise<number>;
  tell?(): number | Promise<number>;
}

export class Metadata {
  public constructor(
    public readonly values: Readonly<Record<string, string>> = {},
  ) {}
}

export class Frame {
  public constructor(
    public readonly kind: FrameKind,
    public readonly data: Uint8Array,
  ) {}
}

export class Track {
  public constructor(
    public readonly id: number,
    public readonly codec?: string,
    public readonly metadata = new Metadata(),
    private readonly essenceFrames: readonly Frame[] = [],
  ) {}

  public *frames(): IterableIterator<Frame> {
    yield* this.essenceFrames;
  }
}

export class Container {
  public constructor(
    public readonly tracks: readonly Track[] = [],
    public readonly metadata = new Metadata(),
    public readonly mode: DecodeMode = "raw",
  ) {}

  public *frames(): IterableIterator<Frame> {
    for (const track of this.tracks) {
      yield* track.frames();
    }
  }
}

export interface DecodeOptions {
  mode?: DecodeMode;
}

/** Decode an open-like source into a lazily traversable container. */
export async function decode(
  source: BinarySource,
  options: DecodeOptions = {},
): Promise<Container> {
  void source;
  decodeNative(options.mode ?? "raw");
  throw new Error("native decoder unexpectedly returned");
}

/** Encode a container to an open-like destination. */
export async function encode(
  container: Container,
  destination: BinarySink,
): Promise<void> {
  void container;
  void destination;
  encodeNative();
}
