import * as assert from "node:assert/strict";
import { test } from "node:test";

import { Container, Frame, Track, decode, encode } from "../dist/index.js";

test("container iterates frames", () => {
  const frames = [
    new Frame("raw_essence", Buffer.from("one")),
    new Frame("raw_essence", Buffer.from("two")),
  ];
  const container = new Container([
    new Track(1, "jpeg2000", undefined, frames),
  ]);

  assert.deepEqual([...container.frames()], frames);
});

test("decode is an explicit scaffold seam", async () => {
  const source = { read: () => new Uint8Array() };
  await assert.rejects(
    decode(source, { mode: "parsed" }),
    /Decode is not implemented/,
  );
});

test("encode is an explicit scaffold seam", async () => {
  const destination = { write: () => 0 };
  await assert.rejects(
    encode(new Container(), destination),
    /Encode is not implemented/,
  );
});
