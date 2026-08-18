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

test("open lists tracks and reads a frame", async (t) => {
  if (!existsSync(fixture)) {
    t.skip("sample_op1a.mxf fixture is missing");
    return;
  }
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
    await clip.close();
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
