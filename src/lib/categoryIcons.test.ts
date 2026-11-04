import { describe, expect, it } from "vitest";
import { CATEGORY_COLORS, CATEGORY_ICONS, ICON_COMPONENTS } from "./categoryIcons";

describe("category icons", () => {
  it("every catalogue entry has a component and an emoji", () => {
    for (const { id, emoji } of CATEGORY_ICONS) {
      expect(ICON_COMPONENTS[id], id).toBeTruthy();
      expect(emoji.length, id).toBeGreaterThan(0);
    }
    expect(Object.keys(ICON_COMPONENTS).sort()).toEqual(CATEGORY_ICONS.map((i) => i.id).sort());
  });

  it("covers the seeded categories", () => {
    for (const id of ["shopping-cart", "utensils", "bus", "car", "house", "wifi", "heart-pulse", "shirt", "lamp",
      "baby", "popcorn", "gift", "repeat", "sparkles", "circle-help", "banknote", "briefcase", "percent"]) {
      expect(CATEGORY_ICONS.some((i) => i.id === id), id).toBe(true);
    }
    expect(new Set(CATEGORY_COLORS).size).toBe(CATEGORY_COLORS.length);
  });
});
