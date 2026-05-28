// src/stabilizer/__tests__/fee-stabilizer.test.ts
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  calculateRequiredFee,
  shouldAdjust,
  applyRateLimit,
} from "../fee-stabilizer";

describe("calculateRequiredFee", () => {
  it("50% fee when gross APY is 2x target", () => {
    assert.equal(calculateRequiredFee(0.08, 400), 5000);
  });
  it("20% fee when gross APY is 1.25x target", () => {
    assert.equal(calculateRequiredFee(0.05, 400), 2000);
  });
  it("0 when gross APY equals target", () => {
    assert.equal(calculateRequiredFee(0.04, 400), 0);
  });
  it("0 when gross APY is below target", () => {
    assert.equal(calculateRequiredFee(0.03, 400), 0);
  });
  it("0 when gross APY is zero", () => {
    assert.equal(calculateRequiredFee(0, 400), 0);
  });
  it("0 when gross APY is negative", () => {
    assert.equal(calculateRequiredFee(-0.02, 400), 0);
  });
  it("9000 when gross=10%, target=1%", () => {
    assert.equal(calculateRequiredFee(0.10, 100), 9000);
  });
});

describe("shouldAdjust", () => {
  it("true when difference exceeds dead zone", () => {
    assert.equal(shouldAdjust(5000, 5100), true);
  });
  it("false when difference is within dead zone", () => {
    assert.equal(shouldAdjust(5000, 5030), false);
  });
  it("false when fees are equal", () => {
    assert.equal(shouldAdjust(5000, 5000), false);
  });
  it("false at the dead-zone boundary (50)", () => {
    assert.equal(shouldAdjust(5000, 5050), false);
  });
  it("true just past the dead-zone boundary (51)", () => {
    assert.equal(shouldAdjust(5000, 5051), true);
  });
});

describe("applyRateLimit", () => {
  it("passes through when delta is within the cap", () => {
    assert.deepEqual(applyRateLimit(1000, 1050), {
      appliedFeeBps: 1050,
      clamped: false,
    });
  });
  it("clamps upward moves to current + max", () => {
    assert.deepEqual(applyRateLimit(1000, 5000), {
      appliedFeeBps: 1100,
      clamped: true,
    });
  });
  it("clamps downward moves to current - max", () => {
    assert.deepEqual(applyRateLimit(2000, 0), {
      appliedFeeBps: 1900,
      clamped: true,
    });
  });
  it("handles equal fees without clamping", () => {
    assert.deepEqual(applyRateLimit(1000, 1000), {
      appliedFeeBps: 1000,
      clamped: false,
    });
  });
});
