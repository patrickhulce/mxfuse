import * as assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { openMxf } from "../dist/index.js";

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
