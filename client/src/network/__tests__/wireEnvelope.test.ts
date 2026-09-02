import { describe, expect, it } from "vitest";

import {
  decodeJsonEnvelope,
  encodeJsonEnvelope,
  WIRE_MAX_DECODED_BYTES,
} from "../wireEnvelope";

describe("wire envelope decoded-size limit", () => {
  it("rejects an oversized raw envelope", async () => {
    const envelope = new Uint8Array(WIRE_MAX_DECODED_BYTES + 2);
    envelope[0] = 0x00;

    await expect(decodeJsonEnvelope(envelope)).rejects.toThrow(
      "wire message exceeds decoded size limit",
    );
  });

  it("rejects highly compressible gzip content above the limit", async () => {
    const json = "x".repeat(WIRE_MAX_DECODED_BYTES + 1);
    const envelope = await encodeJsonEnvelope(json);

    expect(envelope[0]).toBe(0x01);
    expect(envelope.byteLength).toBeLessThan(WIRE_MAX_DECODED_BYTES);
    await expect(decodeJsonEnvelope(envelope)).rejects.toThrow(
      "wire message exceeds decoded size limit",
    );
  });
});
