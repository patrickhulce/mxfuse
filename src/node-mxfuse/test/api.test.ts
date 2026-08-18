import * as assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { EssenceType, Flavour, openMxf, writeMxf } from "../dist/index.js";

const fixture = join(
  process.cwd(),
  "..",
  "..",
  "tests",
  "fixtures",
  "sample_op1a.mxf",
);

test("open lists tracks and reads a frame", async () => {
  assert.ok(
    existsSync(fixture),
    "missing sample_op1a.mxf; run ./scripts/generate-fixture.sh",
  );
  const clip = await openMxf(fixture);
  const info = await clip.info();
  assert.ok(info.duration > 0);
  assert.ok(info.tracks.length > 0);
  const picture = info.tracks.filter((track) => track.kind === "picture");
  await clip.select(picture);
  await clip.seek(0);
  const packages = await clip.read(1);
  assert.ok(packages.length > 0);
  assert.ok(packages[0].frames.length > 0);
  assert.ok(packages[0].frames[0].data.byteLength > 0);
  await clip.close();
});

test("write round trip unc and pcm", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mxfuse-"));
  const path = join(dir, "out.mxf");
  try {
    const picture = Buffer.alloc(1920 * 1080 * 2, 0x11);
    const audio = Buffer.alloc(1920 * 2, 0x33);
    const writer = await writeMxf(path, {
      editRate: [25, 1],
      flavour: Flavour.DEFAULT,
      duration: 1,
      tracks: [
        { essenceType: EssenceType.UNC_HD_1080P },
        {
          essenceType: EssenceType.WAVE_PCM,
          samplingRate: 48000,
          channelCount: 1,
          quantizationBits: 16,
        },
      ],
    });
    await writer.write(0, picture);
    await writer.write(1, audio);
    await writer.finish();

    const clip = await openMxf(path);
    const info = await clip.info();
    assert.equal(info.duration, 1);
    assert.equal(info.tracks.length, 2);
    await clip.select(info.tracks);
    await clip.seek(0);
    const packages = await clip.read(1);
    assert.deepEqual(packages[0].frames[0].data, Uint8Array.from(picture));
    assert.deepEqual(packages[0].frames[1].data, Uint8Array.from(audio));
    assert.ok(packages[0].frames[0].klSize > 0);
    assert.equal(packages[0].frames[0].trackIndex, 0);
    await clip.close();
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("write round trip xml and opaque", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mxfuse-"));
  const path = join(dir, "out.mxf");
  try {
    const xml = Buffer.from('<clip xmlns="urn:x-mxfuse:test">hello</clip>');
    const writer = await writeMxf(path, {
      editRate: [24, 1],
      duration: 2,
      tracks: [
        {
          essenceType: EssenceType.OPAQUE_PICTURE,
          storedWidth: 64,
          storedHeight: 32,
          essenceContainerUl: Uint8Array.from(
            Buffer.from("060e2b34040101010d010301027f0101", "hex"),
          ),
          codingUl: Uint8Array.from(
            Buffer.from("060e2b3404010101040102017f000000", "hex"),
          ),
        },
      ],
      xml: [{ data: xml, language: "en", namespace: "urn:x-mxfuse:test" }],
    });
    await writer.write(0, Buffer.from("frame-a"));
    await writer.write(0, Buffer.from("frame-b"));
    await writer.finish();

    const clip = await openMxf(path);
    const docs = await clip.xml;
    assert.equal(docs.length, 1);
    assert.deepEqual(docs[0].data, Uint8Array.from(xml));
    const info = await clip.info();
    await clip.select(info.tracks);
    await clip.seek(0);
    const packages = await clip.read(2);
    assert.equal(packages.length, 2);
    assert.deepEqual(
      packages[0].frames[0].data,
      Uint8Array.from(Buffer.from("frame-a")),
    );
    assert.deepEqual(
      packages[1].frames[0].data,
      Uint8Array.from(Buffer.from("frame-b")),
    );
    assert.equal(packages[0].frames[0].trackIndex, 0);
    await clip.close();
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
