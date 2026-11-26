import { describe, expect, it } from "vitest";
import { compReduction, gateOpen, limiterReduction } from "./dynamics";

describe("dynamics", () => {
  it("limiter reduces by the overshoot after the bus fader", () => {
    expect(limiterReduction(-3, 6, -1, true)).toBeCloseTo(4);
    expect(limiterReduction(-10, 0, -1, true)).toBe(0);
    expect(limiterReduction(0, 0, -1, false)).toBe(0);
    expect(limiterReduction(-90, 12, -1, true)).toBe(0);
  });
  it("compressor follows its ratio above the threshold", () => {
    expect(compReduction(-10, -20, 4, true)).toBeCloseTo(7.5);
    expect(compReduction(-30, -20, 4, true)).toBe(0);
    expect(compReduction(-10, -20, 1, true)).toBe(0);
    expect(compReduction(-10, -20, 4, false)).toBe(0);
  });
  it("gate opens at the threshold", () => {
    expect(gateOpen(-50, -40, true)).toBe(false);
    expect(gateOpen(-30, -40, true)).toBe(true);
    expect(gateOpen(-50, -40, false)).toBe(true);
  });
});
