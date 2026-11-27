/**
 * Widget ids: static dashboard widgets plus per-account `account_<id>` entries.
 * The three summary tiles (stat_*) and every account card are first-class
 * widgets, so each can be moved and resized on its own.
 */
export type WidgetId = string;

/** Placed widget on the dashboard grid, in grid units. */
export interface GridItem { i: WidgetId; x: number; y: number; w: number; h: number }

/** Flow (reading order) entry — span in 12-column units. */
export interface WidgetLayout { id: WidgetId; span: number }

export const GRID_COLS = 12;
export const ROW_HEIGHT = 24;
export const GRID_MARGIN = 16;

export const ACCOUNT_PREFIX = "account_";
export const accountWidgetId = (id: string): WidgetId => `${ACCOUNT_PREFIX}${id}`;
const isAccountId = (id: string) => id.startsWith(ACCOUNT_PREFIX) && id.length > ACCOUNT_PREFIX.length;

export const STAT_IDS = ["stat_balance", "stat_expense", "stat_income"] as const;
export const CHART_IDS = ["chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople"] as const;

const STATIC_IDS = new Set<string>([...STAT_IDS, ...CHART_IDS, "rates"]);
const isWidgetId = (x: unknown): x is WidgetId => typeof x === "string" && (STATIC_IDS.has(x) || isAccountId(x));
const int = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) ? Math.round(v) : null);

/**
 * minW/minH — hard limits that keep content readable (tables and legends
 * must not overflow sideways). defW — default width; defH — fallback height
 * used until the content has been measured.
 */
export function widgetDef(id: WidgetId): { minW: number; minH: number; defW: number; defH: number } {
  if (isAccountId(id)) return { minW: 3, minH: 3, defW: 4, defH: 4 };
  if (id === "rates") return { minW: 8, minH: 4, defW: 12, defH: 6 };
  if ((STAT_IDS as readonly string[]).includes(id)) return { minW: 3, minH: 3, defW: 4, defH: 3 };
  return { minW: 6, minH: 5, defW: 6, defH: 9 };
}

/** Canonical order: summary tiles, rates, charts, then every account. */
export function defaultOrder(visible: WidgetId[]): WidgetId[] {
  const fixed: WidgetId[] = [...STAT_IDS, "rates", ...CHART_IDS];
  return [...fixed.filter((id) => visible.includes(id)), ...visible.filter(isAccountId)];
}

/** Grid rows needed to show `px` pixels of content. */
export function rowsFor(px: number): number {
  return Math.max(1, Math.ceil((px + GRID_MARGIN) / (ROW_HEIGHT + GRID_MARGIN)));
}

export function clampW(id: WidgetId, w: number): number {
  return Math.max(widgetDef(id).minW, Math.min(GRID_COLS, Math.round(w)));
}

export function clampH(id: WidgetId, h: number): number {
  return Math.max(widgetDef(id).minH, Math.round(h));
}

/** Order + widths of the stock dashboard (used for the default view and reset). */
export function defaultFlow(visible: WidgetId[]): WidgetLayout[] {
  return defaultOrder(visible).map((id) => ({ id, span: widgetDef(id).defW }));
}

/**
 * Pack a reading-order list into grid coordinates row by row, the way a CSS
 * flow grid would. Heights come from measured content when known.
 */
export function flowToGrid(flow: WidgetLayout[], heights: Partial<Record<WidgetId, number>>): GridItem[] {
  const out: GridItem[] = [];
  let x = 0;
  let y = 0;
  let rowH = 0;
  for (const { id, span } of flow) {
    const w = clampW(id, span);
    const h = clampH(id, heights[id] ?? widgetDef(id).defH);
    if (x + w > GRID_COLS) {
      x = 0;
      y += rowH;
      rowH = 0;
    }
    out.push({ i: id, x, y, w, h });
    x += w;
    rowH = Math.max(rowH, h);
  }
  return out;
}

export type ParsedLayout =
  | { kind: "grid"; items: GridItem[] }
  | { kind: "flow"; items: WidgetLayout[] };

/** The old `stats` group expands into three equal tiles inside its own row. */
function expandStats(x: number, y: number, h: number, visible: WidgetId[]): GridItem[] {
  const out: GridItem[] = [];
  STAT_IDS.forEach((sid, k) => {
    if (!visible.includes(sid)) return;
    const d = widgetDef(sid);
    out.push({ i: sid, x: Math.min(x + 4 * k, GRID_COLS - 4), y, w: 4, h: Math.max(d.minH, h) });
  });
  return out;
}

/** The old `accounts` group expands into one widget per visible account. */
function expandAccounts(x: number, y: number, w: number, h: number, visible: WidgetId[]): GridItem[] {
  const out: GridItem[] = [];
  const d = widgetDef("account_x");
  const perRow = Math.max(1, Math.floor(w / 4));
  const accH = Math.max(d.minH, h);
  visible.filter(isAccountId).forEach((id, k) => {
    out.push({
      i: id,
      x: Math.min(x + (k % perRow) * 4, GRID_COLS - 4),
      y: y + Math.floor(k / perRow) * accH,
      w: 4,
      h: accH,
    });
  });
  return out;
}

/** Legacy v2 (4-column, group ids) → native 12-column items. */
function expandLegacyGrid(src: unknown, visible: WidgetId[]): GridItem[] {
  const out: GridItem[] = [];
  if (!Array.isArray(src)) return out;
  for (const it of src) {
    const o = it as Record<string, unknown>;
    const id = o?.i;
    const x = int(o?.x), y = int(o?.y), w = int(o?.w), h = int(o?.h);
    if (typeof id !== "string" || x == null || y == null || w == null || h == null) continue;
    const gx = Math.max(0, x) * 3, gy = Math.max(0, y), gw = Math.max(1, w) * 3;
    if (id === "stats") out.push(...expandStats(gx, gy, h, visible));
    else if (id === "accounts") out.push(...expandAccounts(gx, gy, gw, h, visible));
    else if (isWidgetId(id) && visible.includes(id) && !out.some((e) => e.i === id)) {
      const cw = clampW(id, gw);
      out.push({ i: id, x: Math.min(gx, GRID_COLS - cw), y: gy, w: cw, h: clampH(id, h) });
    }
  }
  return out;
}

/** Legacy v1 (4-column flow, group ids) → flow entries in 12-column spans. */
function expandLegacyFlow(src: unknown[], visible: WidgetId[]): WidgetLayout[] {
  const out: WidgetLayout[] = [];
  for (const it of src) {
    const id = (it as { id?: unknown })?.id;
    const span = Math.max(1, Math.round(Number((it as { span?: unknown })?.span) || 1)) * 3;
    if (id === "stats") {
      for (const sid of STAT_IDS) {
        if (visible.includes(sid) && !out.some((e) => e.id === sid)) out.push({ id: sid, span: 4 });
      }
    } else if (id === "accounts") {
      for (const acc of visible.filter(isAccountId)) {
        if (!out.some((e) => e.id === acc)) out.push({ id: acc, span: 4 });
      }
    } else if (isWidgetId(id) && visible.includes(id) && !out.some((e) => e.id === id)) {
      out.push({ id, span: clampW(id, span) });
    }
  }
  return out;
}

/** Visible widgets absent from the list are appended below it. */
function appendMissingGrid(items: GridItem[], visible: WidgetId[]): GridItem[] {
  let bottom = items.reduce((m, e) => Math.max(m, e.y + e.h), 0);
  for (const id of defaultOrder(visible)) {
    if (items.some((e) => e.i === id)) continue;
    const d = widgetDef(id);
    items.push({ i: id, x: 0, y: bottom, w: d.defW, h: d.defH });
    bottom += d.defH;
  }
  return items;
}

/**
 * Read `settings.dash_layout`.
 * - v3 `{"v":3,"items":[{i,x,y,w,h}]}` → saved 12-column grid (fixed heights).
 * - v2 `{"v":2,...}` (0.3.x builds) → legacy 4-column grid, expanded ×3 with
 *   group widgets split into their individual elements.
 * - v1 `[{id,span}]` → legacy flow order/widths, same expansion, auto-fit heights.
 * - empty / broken → stock flow, heights auto-fit.
 * Unknown and hidden widgets are dropped; visible widgets missing from a
 * saved grid are appended below it.
 */
export function parseLayout(raw: string | undefined, visible: WidgetId[]): ParsedLayout {
  let parsed: unknown;
  try {
    parsed = raw ? JSON.parse(raw) : null;
  } catch {
    parsed = null;
  }

  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    const v = (parsed as { v?: unknown }).v;
    const src = (parsed as { items?: unknown }).items;
    if (v === 3 && Array.isArray(src)) {
      const items: GridItem[] = [];
      for (const it of src) {
        const o = it as Record<string, unknown>;
        const id = o?.i;
        const x = int(o?.x), y = int(o?.y), w = int(o?.w), h = int(o?.h);
        if (!isWidgetId(id) || !visible.includes(id) || items.some((e) => e.i === id)) continue;
        if (x == null || y == null || w == null || h == null) continue;
        const cw = clampW(id, w);
        items.push({ i: id, x: Math.max(0, Math.min(GRID_COLS - cw, x)), y: Math.max(0, y), w: cw, h: clampH(id, h) });
      }
      if (items.length > 0) return { kind: "grid", items: appendMissingGrid(items, visible) };
    }
    if (v === 2) {
      const items = expandLegacyGrid(src, visible);
      if (items.length > 0) return { kind: "grid", items: appendMissingGrid(items, visible) };
    }
  }

  if (Array.isArray(parsed)) {
    const flow = expandLegacyFlow(parsed, visible);
    for (const w of defaultFlow(visible)) if (!flow.some((e) => e.id === w.id)) flow.push(w);
    if (flow.length > 0) return { kind: "flow", items: flow };
  }

  return { kind: "flow", items: defaultFlow(visible) };
}

export function serializeGrid(items: GridItem[]): string {
  return JSON.stringify({ v: 3, items: items.map(({ i, x, y, w, h }) => ({ i, x, y, w, h })) });
}

/** Items sorted top-to-bottom, left-to-right — the reading order. */
export function readingOrder(items: GridItem[]): GridItem[] {
  return [...items].sort((a, b) => a.y - b.y || a.x - b.x);
}
