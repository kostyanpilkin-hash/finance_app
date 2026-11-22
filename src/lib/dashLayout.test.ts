import { describe, expect, it } from "vitest";
import { clampSpan, defaultLayout, normalizeLayout, type WidgetId } from "./dashLayout";

const ALL: WidgetId[] = [
  "stats", "rates", "accounts",
  "chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople",
];

describe("defaultLayout", () => {
  it("returns the current dashboard arrangement", () => {
    const l = defaultLayout(ALL);
    expect(l.map((w) => w.id)).toEqual([
      "stats", "chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople", "rates", "accounts",
    ]);
    expect(l.find((w) => w.id === "stats")?.span).toBe(4);
    expect(l.find((w) => w.id === "chart_pieMonth")?.span).toBe(2);
  });

  it("skips invisible widgets", () => {
    const l = defaultLayout(["stats", "accounts"]);
    expect(l).toEqual([{ id: "stats", span: 4 }, { id: "accounts", span: 4 }]);
  });
});

describe("normalizeLayout", () => {
  it("returns default when unset or broken", () => {
    expect(normalizeLayout(undefined, ALL)).toEqual(defaultLayout(ALL));
    expect(normalizeLayout("{{{", ALL)).toEqual(defaultLayout(ALL));
    expect(normalizeLayout("{}", ALL)).toEqual(defaultLayout(ALL));
  });

  it("preserves user order and spans", () => {
    const l = normalizeLayout('[{"id":"rates","span":3},{"id":"stats","span":4}]', ALL);
    expect(l[0]).toEqual({ id: "rates", span: 3 });
    expect(l[1]).toEqual({ id: "stats", span: 4 });
  });

  it("drops hidden and unknown widgets", () => {
    const l = normalizeLayout('[{"id":"chart_incomeByCat","span":2},{"id":"nope","span":4}]', ["stats"]);
    expect(l).toEqual([{ id: "stats", span: 4 }]);
  });

  it("clamps span to widget bounds", () => {
    const l = normalizeLayout('[{"id":"chart_pieMonth","span":1},{"id":"stats","span":1},{"id":"accounts","span":9}]', ALL);
    expect(l.find((w) => w.id === "chart_pieMonth")?.span).toBe(2);
    expect(l.find((w) => w.id === "stats")?.span).toBe(3);
    expect(l.find((w) => w.id === "accounts")?.span).toBe(4);
  });

  it("appends missing visible widgets with defaults", () => {
    const l = normalizeLayout('[{"id":"stats","span":3}]', ALL);
    expect(l).toHaveLength(ALL.length);
    expect(l[l.length - 1]?.id).toBe("accounts");
    expect(l.find((w) => w.id === "rates")?.span).toBe(4);
  });

  it("dedupes repeated ids", () => {
    const l = normalizeLayout('[{"id":"stats","span":4},{"id":"stats","span":3}]', ["stats"]);
    expect(l).toEqual([{ id: "stats", span: 4 }]);
  });
});

describe("clampSpan", () => {
  it("respects per-widget min and grid max", () => {
    expect(clampSpan("chart_pieMonth", 1)).toBe(2);
    expect(clampSpan("stats", 2)).toBe(3);
    expect(clampSpan("rates", 2)).toBe(3);
    expect(clampSpan("accounts", 1)).toBe(2);
    expect(clampSpan("accounts", 5)).toBe(4);
    expect(clampSpan("chart_pieMonth", 3)).toBe(3);
  });
});
