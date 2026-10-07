import { describe, expect, test } from "bun:test";
import { formatTick, linearTicks, logTicks, niceBounds, niceStep } from "../src/plot/ticks.ts";

describe("ticks", () => {
  test("niceStep", () => {
    expect(niceStep(0.9)).toBe(1);
    expect(niceStep(2.2)).toBe(2);
    expect(niceStep(4)).toBe(5);
    expect(niceStep(8)).toBe(10);
    expect(niceStep(0.023)).toBeCloseTo(0.02, 12);
  });
  test("linear ticks lie in range and are evenly spaced", () => {
    const t = linearTicks(0, 1, 5);
    expect(t.ticks).toEqual([0, 0.2, 0.4, 0.6, 0.8, 1]);
    const u = linearTicks(-3.3, 7.1, 5);
    expect(u.ticks[0]!).toBeGreaterThanOrEqual(-3.3);
    expect(u.ticks.at(-1)!).toBeLessThanOrEqual(7.1);
  });
  test("degenerate range", () => {
    expect(linearTicks(2, 2).ticks.length).toBeGreaterThan(1);
  });
  test("niceBounds", () => {
    expect(niceBounds(0.03, 0.97, 5)).toEqual([0, 1]);
  });
  test("log ticks", () => {
    const t = logTicks(10, 1e6);
    expect(t.major).toEqual([10, 100, 1000, 1e4, 1e5, 1e6]);
    expect(t.minor).toContain(20);
    expect(logTicks(0, 5).major).toEqual([]);
    expect(logTicks(3, 40).major).toEqual([10]);
  });
  test("format", () => {
    expect(formatTick(0)).toBe("0");
    expect(formatTick(0.0005)).toBe("500µ");
    expect(formatTick(1e5)).toBe("100k");
    expect(formatTick(0.30000000000000004)).toBe("300m");
  });
});
