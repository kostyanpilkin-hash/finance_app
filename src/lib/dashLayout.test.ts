import { describe, expect, it } from "vitest";
import {
  accountWidgetId, clampH, clampW, defaultFlow, flowToGrid, parseLayout, readingOrder,
  rowsFor, serializeGrid, widgetDef, type GridItem, type WidgetId,
} from "./dashLayout";

const ACC1 = accountWidgetId("acc-1");
const ACC2 = accountWidgetId("acc-2");

const ALL: WidgetId[] = [
  "stat_balance", "stat_expense", "stat_income", "rates",
  "chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople",
  ACC1, ACC2,
];

describe("defaultFlow", () => {
  it("keeps the stock order: summary tiles, rates, charts, accounts", () => {
    expect(defaultFlow(ALL).map((w) => w.id)).toEqual([
      "stat_balance", "stat_expense", "stat_income", "rates",
      "chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople",
      ACC1, ACC2,
    ]);
  });

  it("gives the three summary tiles one full 12-column row", () => {
    const spans = defaultFlow(["stat_balance", "stat_expense", "stat_income"]).map((w) => w.span);
    expect(spans).toEqual([4, 4, 4]);
  });
});

describe("flowToGrid", () => {
  it("packs rows like a 12-column flow grid using measured heights", () => {
    const g = flowToGrid(defaultFlow(ALL), { stat_balance: 3, rates: 6, chart_pieMonth: 10, chart_incomeByCat: 7 });
    expect(g.find((i) => i.i === "stat_balance")).toEqual({ i: "stat_balance", x: 0, y: 0, w: 4, h: 3 });
    expect(g.find((i) => i.i === "stat_income")).toEqual({ i: "stat_income", x: 8, y: 0, w: 4, h: 3 });
    expect(g.find((i) => i.i === "rates")).toEqual({ i: "rates", x: 0, y: 3, w: 12, h: 6 });
    expect(g.find((i) => i.i === "chart_pieMonth")).toEqual({ i: "chart_pieMonth", x: 0, y: 9, w: 6, h: 10 });
    expect(g.find((i) => i.i === "chart_incomeByCat")).toEqual({ i: "chart_incomeByCat", x: 6, y: 9, w: 6, h: 7 });
    // next row starts below the taller card of the previous row
    expect(g.find((i) => i.i === "chart_expenseByPeople")?.y).toBe(19);
    expect(g.find((i) => i.i === ACC1)).toEqual({ i: ACC1, x: 0, y: 28, w: 4, h: 4 });
    expect(g.find((i) => i.i === ACC2)).toEqual({ i: ACC2, x: 4, y: 28, w: 4, h: 4 });
  });

  it("never lets a measured height go below the widget minimum", () => {
    expect(flowToGrid([{ id: "chart_pieMonth", span: 6 }], { chart_pieMonth: 1 })[0].h).toBe(5);
  });
});

describe("parseLayout", () => {
  it("falls back to the stock flow when unset or broken", () => {
    for (const raw of [undefined, "", "{{{", "{}", '{"v":3,"items":[]}', '{"v":9,"items":[]}']) {
      expect(parseLayout(raw, ALL)).toEqual({ kind: "flow", items: defaultFlow(ALL) });
    }
  });

  it("reads a saved v3 grid and clamps it into bounds", () => {
    const raw = JSON.stringify({ v: 3, items: [
      { i: "rates", x: 9, y: 0, w: 12, h: 1 },
      { i: "chart_pieMonth", x: -2, y: -5, w: 1, h: 20 },
    ] });
    const p = parseLayout(raw, ["rates", "chart_pieMonth"]);
    expect(p.kind).toBe("grid");
    const items = (p as { items: GridItem[] }).items;
    expect(items[0]).toEqual({ i: "rates", x: 0, y: 0, w: 12, h: 4 });
    expect(items[1]).toEqual({ i: "chart_pieMonth", x: 0, y: 0, w: 6, h: 20 });
  });

  it("drops hidden/unknown/duplicate widgets and appends missing visible ones below", () => {
    const raw = JSON.stringify({ v: 3, items: [
      { i: "stat_balance", x: 0, y: 0, w: 4, h: 3 },
      { i: "stat_balance", x: 0, y: 9, w: 4, h: 3 },
      { i: "nope", x: 0, y: 0, w: 1, h: 1 },
      { i: ACC1, x: 0, y: 3, w: 4, h: 4 },
    ] });
    const p = parseLayout(raw, ["stat_balance", "stat_expense", ACC1, accountWidgetId("gone")]) as { kind: string; items: GridItem[] };
    expect(p.kind).toBe("grid");
    expect(p.items).toEqual([
      { i: "stat_balance", x: 0, y: 0, w: 4, h: 3 },
      { i: ACC1, x: 0, y: 3, w: 4, h: 4 },
      { i: "stat_expense", x: 0, y: 7, w: 4, h: 3 },
      { i: accountWidgetId("gone"), x: 0, y: 10, w: 4, h: 4 },
    ]);
  });

  it("migrates a legacy v2 4-column grid, splitting group widgets", () => {
    // The layout the user actually saved: rates on top, stats row, charts.
    const raw = JSON.stringify({ v: 2, items: [
      { i: "rates", x: 0, y: 0, w: 4, h: 6 },
      { i: "stats", x: 0, y: 6, w: 4, h: 3 },
      { i: "chart_pieMonth", x: 0, y: 9, w: 2, h: 9 },
      { i: "chart_incomeByCat", x: 2, y: 9, w: 2, h: 9 },
      { i: "accounts", x: 0, y: 18, w: 4, h: 4 },
    ] });
    const p = parseLayout(raw, ALL);
    expect(p.kind).toBe("grid");
    const items = (p as { items: GridItem[] }).items;
    expect(items.find((i) => i.i === "rates")).toEqual({ i: "rates", x: 0, y: 0, w: 12, h: 6 });
    expect(items.find((i) => i.i === "stat_balance")).toEqual({ i: "stat_balance", x: 0, y: 6, w: 4, h: 3 });
    expect(items.find((i) => i.i === "stat_expense")).toEqual({ i: "stat_expense", x: 4, y: 6, w: 4, h: 3 });
    expect(items.find((i) => i.i === "stat_income")).toEqual({ i: "stat_income", x: 8, y: 6, w: 4, h: 3 });
    expect(items.find((i) => i.i === "chart_pieMonth")).toEqual({ i: "chart_pieMonth", x: 0, y: 9, w: 6, h: 9 });
    expect(items.find((i) => i.i === "chart_incomeByCat")).toEqual({ i: "chart_incomeByCat", x: 6, y: 9, w: 6, h: 9 });
    expect(items.find((i) => i.i === ACC1)).toEqual({ i: ACC1, x: 0, y: 18, w: 4, h: 4 });
    expect(items.find((i) => i.i === ACC2)).toEqual({ i: ACC2, x: 4, y: 18, w: 4, h: 4 });
    // No overlaps anywhere.
    for (const a of items) for (const b of items) {
      if (a === b) continue;
      const overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
      expect(overlap, `${a.i} vs ${b.i}`).toBe(false);
    }
  });

  it("drops accounts that no longer exist during v2 migration", () => {
    const raw = JSON.stringify({ v: 2, items: [{ i: "accounts", x: 0, y: 0, w: 4, h: 4 }] });
    const p = parseLayout(raw, [...ALL.filter((i) => !i.startsWith("account_")), ACC1]) as { items: GridItem[] };
    expect(p.items.map((i) => i.i)).toContain(ACC1);
    expect(p.items.map((i) => i.i)).not.toContain(ACC2);
  });

  it("migrates the legacy v1 list as flow order and widths, splitting groups", () => {
    const p = parseLayout('[{"id":"rates","span":3},{"id":"stats","span":4}]', ["stat_balance", "stat_expense", "stat_income", "rates", ACC1]);
    expect(p).toEqual({ kind: "flow", items: [
      { id: "rates", span: 9 },
      { id: "stat_balance", span: 4 }, { id: "stat_expense", span: 4 }, { id: "stat_income", span: 4 },
      { id: ACC1, span: 4 },
    ] });
  });

  it("round-trips through serializeGrid", () => {
    const items: GridItem[] = [{ i: "stat_balance", x: 0, y: 0, w: 4, h: 3 }, { i: ACC1, x: 4, y: 0, w: 8, h: 4 }];
    expect(parseLayout(serializeGrid(items), ["stat_balance", ACC1])).toEqual({ kind: "grid", items });
  });
});

describe("helpers", () => {
  it("converts pixels to grid rows (24px rows, 16px gaps)", () => {
    expect(rowsFor(24)).toBe(1);
    expect(rowsFor(64)).toBe(2);
    expect(rowsFor(65)).toBe(3);
    expect(rowsFor(0)).toBe(1);
  });

  it("clamps sizes to widget limits", () => {
    expect(clampW("chart_pieMonth", 1)).toBe(6);
    expect(clampW("stat_balance", 2)).toBe(3);
    expect(clampW("rates", 6)).toBe(8);
    expect(clampW(ACC1, 99)).toBe(12);
    expect(clampH("rates", 1)).toBe(4);
    expect(clampH("rates", 12)).toBe(12);
    expect(widgetDef(ACC1)).toEqual(widgetDef("account_anything"));
  });

  it("sorts items in reading order", () => {
    const items: GridItem[] = [
      { i: "rates", x: 2, y: 3, w: 6, h: 4 }, { i: "stat_balance", x: 0, y: 0, w: 4, h: 3 }, { i: ACC1, x: 0, y: 3, w: 2, h: 4 },
    ];
    expect(readingOrder(items).map((i) => i.i)).toEqual(["stat_balance", ACC1, "rates"]);
  });
});
