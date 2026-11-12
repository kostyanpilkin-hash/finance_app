import { describe, expect, it } from "vitest";
import { convertMinorLocal, formatMinor, parseMinor } from "./money";

const RATES = [
  { currency: "USD", rateBynScaled: 3_250_000, scale: 1 },
  { currency: "RUB", rateBynScaled: 3_600_000, scale: 100 }, // 3.60 BYN per 100 RUB
];

describe("convertMinorLocal", () => {
  it("same currency is identity", () => {
    expect(convertMinorLocal(1234, "USD", "USD", RATES)).toBe(1234);
  });

  it("converts through BYN", () => {
    expect(convertMinorLocal(10000, "USD", "BYN", RATES)).toBe(32500);
    expect(convertMinorLocal(10000, "RUB", "BYN", RATES)).toBe(360);
  });

  it("rounds to the kopeck instead of truncating", () => {
    // 100.00 BYN at 3.25 → 30.7692 USD → 3077 (truncation gave 3076).
    expect(convertMinorLocal(10000, "BYN", "USD", RATES)).toBe(3077);
    expect(convertMinorLocal(100, "BYN", "USD", RATES)).toBe(31); // 30.77
  });

  it("returns null when a rate is missing", () => {
    expect(convertMinorLocal(100, "EUR", "BYN", RATES)).toBeNull();
    expect(convertMinorLocal(100, "USD", "EUR", RATES)).toBeNull();
  });
});

describe("parseMinor / formatMinor", () => {
  it("parses both separators", () => {
    expect(parseMinor("12,5")).toBe(1250);
    expect(parseMinor("1 234.56")).toBe(123456);
    expect(parseMinor("abc")).toBeNull();
    expect(parseMinor("")).toBeNull();
  });

  it("formats with grouping", () => {
    expect(formatMinor(123456, "BYN")).toBe("1 234.56 BYN");
    expect(formatMinor(-50, "BYN")).toBe("-0.50 BYN");
  });
});
