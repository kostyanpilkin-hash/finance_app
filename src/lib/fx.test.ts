import { describe, expect, it } from "vitest";
import { deviationPct, effectiveRate, formatRate, nextRateSource, rateUnits } from "./fx";

describe("effectiveRate", () => {
  it("finds the BYN side", () => {
    expect(effectiveRate("USD", 1000, "BYN", 2921)).toBe(2_921_000);
    expect(effectiveRate("BYN", 3010, "USD", 1000)).toBe(3_010_000);
  });
  it("is null for cross pairs and empty amounts", () => {
    expect(effectiveRate("USD", 1000, "EUR", 900)).toBeNull();
    expect(effectiveRate("USD", 0, "BYN", 900)).toBeNull();
  });
});

describe("deviationPct", () => {
  it("rounds to one decimal", () => {
    expect(deviationPct(2_921_000, 2_990_500)).toBe(-2.3);
    expect(deviationPct(2_990_500, 2_990_500)).toBe(0);
    expect(deviationPct(1, 0)).toBe(0);
  });
});

describe("nextRateSource", () => {
  it("keeps the filled kind while the amount matches", () => {
    expect(nextRateSource({ amount: 1673, kind: "bsb_app" }, 1673)).toBe("bsb_app");
  });
  it("becomes manual once edited or typed from scratch", () => {
    expect(nextRateSource({ amount: 1673, kind: "bsb_app" }, 1680)).toBe("manual");
    expect(nextRateSource(null, 1680)).toBe("manual");
  });
  it("is null for an empty field", () => {
    expect(nextRateSource(null, null)).toBeNull();
    expect(nextRateSource({ amount: 1, kind: "nbrb" }, 0)).toBeNull();
  });
});

describe("formatRate", () => {
  it("quotes RUB per 100", () => {
    expect(rateUnits("rub")).toBe(100);
    expect(formatRate(34_753, "RUB", "en")).toBe("3.4753");
    expect(formatRate(2_988_000, "USD", "en")).toBe("2.9880");
  });
});
