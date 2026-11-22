export type WidgetId =
  | "stats" | "rates" | "accounts"
  | "chart_pieMonth" | "chart_incomeByCat" | "chart_expenseByPeople" | "chart_incomeByPeople";

export interface WidgetLayout { id: WidgetId; span: number }

export const GRID_COLS = 4;

/** min — жёсткий минимум ширины (защита от переполнения), def — дефолтный span. */
export const WIDGET_DEFS: Record<WidgetId, { min: number; def: number }> = {
  stats: { min: 3, def: 4 },
  rates: { min: 3, def: 4 },
  accounts: { min: 2, def: 4 },
  chart_pieMonth: { min: 2, def: 2 },
  chart_incomeByCat: { min: 2, def: 2 },
  chart_expenseByPeople: { min: 2, def: 2 },
  chart_incomeByPeople: { min: 2, def: 2 },
};

const IDS = new Set(Object.keys(WIDGET_DEFS));
const isWidgetId = (x: unknown): x is WidgetId => typeof x === "string" && IDS.has(x);

export function clampSpan(id: WidgetId, span: number): number {
  const d = WIDGET_DEFS[id];
  return Math.max(d.min, Math.min(GRID_COLS, Math.round(span)));
}

/** Дефолтная раскладка = текущий вид дашборда. */
export function defaultLayout(visible: WidgetId[]): WidgetLayout[] {
  const order: WidgetId[] = ["stats", "chart_pieMonth", "chart_incomeByCat", "chart_expenseByPeople", "chart_incomeByPeople", "rates", "accounts"];
  return order.filter((id) => visible.includes(id)).map((id) => ({ id, span: WIDGET_DEFS[id].def }));
}

/**
 * Разбор `settings.dash_layout`: невалидный JSON/неизвестные id/невидимые
 * виджеты выкидываются, недостающие видимые дописываются в конец с дефолтом.
 */
export function normalizeLayout(raw: string | undefined, visible: WidgetId[]): WidgetLayout[] {
  const out: WidgetLayout[] = [];
  try {
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (Array.isArray(parsed)) {
      for (const item of parsed) {
        const id = (item as { id?: unknown })?.id;
        if (isWidgetId(id) && visible.includes(id) && !out.some((w) => w.id === id)) {
          out.push({ id, span: clampSpan(id, Number((item as { span?: unknown }).span) || WIDGET_DEFS[id].def) });
        }
      }
    }
  } catch { /* игнорируем битый JSON */ }
  for (const w of defaultLayout(visible)) if (!out.some((o) => o.id === w.id)) out.push(w);
  return out;
}
