import { describe, expect, test } from "bun:test";
import { formatSI, parseSI } from "../src/circuit/si.ts";

describe("SI", () => {
  test("format", () => {
    expect(formatSI(0)).toBe("0");
    expect(formatSI(1500)).toBe("1.5k");
    expect(formatSI(0.0025, 3, "V")).toBe("2.5 mV");
    expect(formatSI(1e-6)).toBe("1µ");
    expect(formatSI(999.96, 3)).toBe("1k");
    expect(formatSI(-4.7e-9, 3, "F")).toBe("-4.7 nF");
    expect(formatSI(2.2e6, 3, "Hz")).toBe("2.2 MHz");
    expect(formatSI(12.345, 3)).toBe("12.3");
  });
  test("parse", () => {
    expect(parseSI("1k")).toBe(1000);
    expect(parseSI("10u")).toBeCloseTo(1e-5, 12);
    expect(parseSI("1meg")).toBe(1e6);
    expect(parseSI("1MEG")).toBe(1e6);
    expect(parseSI("2.2n")).toBeCloseTo(2.2e-9, 15);
    expect(parseSI("5")).toBe(5);
    expect(parseSI("1e-3")).toBe(0.001);
    expect(parseSI("10uF")).toBeCloseTo(1e-5, 12);
    expect(parseSI("3m")).toBe(0.003);
    expect(parseSI("abc")).toBeNull();
    expect(parseSI("{x}")).toBeNull();
  });
});
