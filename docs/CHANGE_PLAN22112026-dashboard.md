# План: режим редактирования дашборда (перетаскивание и ресайз виджетов)

> Для исполнителя: план написан так, чтобы его можно было выполнить без знания кодовой базы. Сначала прочитай раздел «Как устроено сейчас», затем делай задачи по порядку. Каждая задача — отдельный коммит. Проверки после каждой: `npx tsc --noEmit`, `npm run lint`, `npm test`; в конце — `npm run build`.

## Решения (утверждено пользователем)

- Все виджеты дашборда переставляемы и растягиваемы: сводка (3 стат-карточки одним блоком), курсы, каждая диаграмма, блок счетов — 7 виджетов.
- Явный режим редактирования: кнопка-карандаш в заголовке → пунктирные рамки, грип-иконка, уголок-ручка; «Готово» / «Сбросить вид».
- Сетка 4 колонки, виджет занимает 1–4 колонки (span). Минимумы: stats 3, rates 3, диаграммы 2, accounts 2. Высота авто по контенту. Дырки в сетке допустимы (виджет не влез в хвост строки — ушёл на следующую).
- Раскладка локальная на устройстве: `settings.dash_layout` (JSON), действует только при ширине ≥ lg (1024px); ниже — текущий статичный порядок.
- Своя реализация без зависимостей, «полный функционал»: живой призрак за курсором, виджеты разъезжаются при наведении, FLIP-анимации, кнопки-альтернативы drag («‹ › − +»).
- Сохранение мгновенное на каждое завершённое действие; «Сбросить вид» возвращает дефолт.
- Редактируются только видимые виджеты; включение/выключение диаграмм остаётся в Настройках (`dash_chart_*`, `dash_rates`).

## Как устроено сейчас

- `src/pages/DashboardPage.tsx` — три независимые сетки: стат-карточки (`grid sm:grid-cols-3`, ~строка 135), полоса курсов (`RatesCard`), диаграммы (`grid lg:grid-cols-2`, 4 карточки по флажкам `dash_chart_income_cat`, `dash_chart_expense_people`, `dash_chart_income_people` + всегда-включённый `pieMonth`), счета (`grid sm:grid-cols-2 lg:grid-cols-3`, ~строка 191).
- Видимость: `chartOn(key)` читает `settings.data?.[key] ?? "1"`; `dash_rates` — `ratesOn`.
- Настройки: `useSettings()` (`src/lib/queries.ts`) → `qk.settings`; запись через `useLocalSettings()` (`src/pages/settings/useLocalSettings.ts`): `set(key, value)` → `api.setSetting` + инвалидация `qk.settings`.
- Тесты vitest лежат рядом: `src/lib/*.test.ts`.
- Иконки: `lucide-react`. Утилита `cn` из `@/lib/utils`.
- Локали: `src/i18n/locales/{ru,be,en}.ts`, тип `Locale = typeof ru` — во всех трёх файлах ключи обязаны совпадать. Новые ключи кладём в блок `ui` (см. `ui.journal` — он вложен внутрь `ui`, обращение `t("ui.journal.*")`).

## Модель данных

`settings.dash_layout` — строка JSON:

```json
[{"id":"stats","span":4},{"id":"chart_pieMonth","span":2},{"id":"chart_incomeByCat","span":2},{"id":"rates","span":4},{"id":"accounts","span":4}]
```

Порядок массива = порядок виджетов (flow-сетка: виджет не влез в остаток строки — переносится, дырка остаётся). Отсутствующие видимые виджеты дописываются в конец с дефолтным span; неизвестные/невидимые id выкидываются; span клампится в `[min, 4]`.

---

### Задача 1: модель раскладки `src/lib/dashLayout.ts` + тесты

**Файлы:**
- Create: `src/lib/dashLayout.ts`
- Create: `src/lib/dashLayout.test.ts`

Содержимое `dashLayout.ts`:

```ts
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
  for (const id of visible) if (!out.some((w) => w.id === id)) out.push({ id, span: WIDGET_DEFS[id].def });
  return out;
}
```

Тесты `dashLayout.test.ts` (vitest, стиль — как в `src/lib/money.test.ts`):

- `normalize_layout_returns_default_when_unset` — `undefined` + все 7 видимых → дефолтный порядок и span'ы.
- `normalize_layout_preserves_user_order_and_spans` — сохранённая `[{rates,4},{stats,4}]` → rates первым.
- `normalize_layout_drops_hidden_and_unknown` — в списке `chart_incomeByCat` и `nope`, видимых нет chart_incomeByCat → выкинуты оба.
- `normalize_layout_clamps_span_to_widget_min` — `{chart_pieMonth, span:1}` → span 2; `{stats, span:1}` → 3; `{accounts, span:9}` → 4.
- `normalize_layout_appends_missing_visible` — в списке только stats → остальные видимые добавлены с дефолтом в конец.
- `normalize_layout_tolerates_broken_json` — `"{{{"` → дефолт.
- `clamp_span_bounds` — min/max.

Проверка: `npm test` — зелёное. Коммит: `feat(dash): widget layout model`.

### Задача 2: хук `src/lib/useMediaQuery.ts`

```ts
import { useEffect, useState } from "react";

export function useMediaQuery(query: string): boolean {
  const [match, setMatch] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const fn = () => setMatch(mq.matches);
    mq.addEventListener("change", fn);
    return () => mq.removeEventListener("change", fn);
  }, [query]);
  return match;
}
```

Коммит вместе с задачей 3 (без отдельного теста — тривиально, jsdom окружение может не иметь matchMedia; если упадёт — полифил в тесте не нужен, хук тестами не покрываем).

### Задача 3: `src/components/dashboard/DashGrid.tsx` — редактируемая сетка

**Интерфейс:**

```ts
interface DashGridProps {
  layout: WidgetLayout[];                 // текущая раскладка (уже normalizeLayout)
  editing: boolean;
  renderWidget: (id: WidgetId) => ReactNode;
  widgetTitle: (id: WidgetId) => string;  // имя для aria-label/чипа
  onChange: (layout: WidgetLayout[]) => void; // вызывается на каждое завершённое изменение
}
```

**Рендер:** `<div className="grid grid-cols-4 gap-4">`, каждый виджет — `<div style={{ gridColumn: `+"`span N / span N`"+` }} className="min-w-0 relative">` + содержимое через `renderWidget(id)`. Ссылки на DOM-ноды — `Map<WidgetId, HTMLElement>` через callback-ref (`itemRefs.current.set(id, el)`).

**Edit mode (editing=true):**
- Каждый виджет: `outline outline-2 outline-dashed outline-primary/40 rounded-2xl -outline-offset-2` (или аналог через border); содержимое `pointer-events-none select-none` (отключает клики по диаграммам).
- Грип-иконка `GripVertical` (lucide) в правом верхнем углу поверх карточки (`absolute right-2 top-2 text-muted-foreground`).
- Ручка ресайза: `absolute -bottom-1 -right-1 h-5 w-5 cursor-nwse-resize` с визуальным «уголком» (два border-шеврона или `GripHorizontal` повёрнутый — проще: `div` с `border-b-2 border-r-2 border-primary`).
- Кнопки-альтернативы (левый верхний угол, `flex gap-0.5`, кнопки `h-6 w-6 rounded-md bg-card border`): `‹` moveEarlier (disabled на первом), `›` moveLater (disabled на последнем), `−` span−1 (disabled на min), `+` span+1 (disabled на 4). Каждая вызывает `onChange` с новым массивом. aria-labels через `widgetTitle` + i18n.
- Чип имени виджета: `absolute left-2 top-2` — нет, там кнопки; имя — в `title`/aria, чип не обязателен. Можно чип снизу слева — решать по месту, чтобы не наезжал на кнопки: кнопки слева сверху, грип справа сверху, чип имени под кнопками или внизу слева (`text-[10px] text-muted-foreground`).

**Drag (pointer events, без библиотек):**
- `onPointerDown` на обёртке виджета (только editing, кнопка `button[0]`). Записать `{id, startX, startY, rect}` в ref; `setPointerCapture`.
- `onPointerMove`: до 6px — ничего; после — `dragging = {id, offX, offY}` в state. Призрак: `position: fixed`, `left: e.clientX - offX`, `top: e.clientY - offY`, `width/height` из rect, `pointer-events-none opacity-80 shadow-xl z-50`, внутри — `renderWidget(id)` (рендерится второй раз — донаты/таблицы лёгкие, ок). Оригинал в потоке заменяем плейсхолдером: `border-2 border-dashed border-primary/50 rounded-2xl` той же высоты.
- Целевой индекс: по `e.clientX/Y` — для каждого виджета считаем `rect`; индекс = количество виджетов, у которых `centerY < pointerY` ИЛИ (та же строка: `|centerY - pointerY| < rowH/2` И `centerX < pointerX`). Проще и надёжнее: `index = items.filter(i => i.centerY < p.y || (i.centerY ~ p.y && i.centerX < p.x)).length`. При изменении индекса — локальный `preview` порядок (state `dragOrder`), виджеты перестраиваются (FLIP анимирует).
- `onPointerUp`: `onChange(dragOrder)`, сброс состояний.

**FLIP-анимация (useLayoutEffect):**
```ts
const rects = useRef<Map<WidgetId, DOMRect>>(new Map());
useLayoutEffect(() => {
  const next = new Map<WidgetId, DOMRect>();
  for (const [id, el] of itemRefs.current) {
    const n = el.getBoundingClientRect();
    const o = rects.current.get(id);
    if (o && (o.left !== n.left || o.top !== n.top)) {
      el.style.transition = "none";
      el.style.transform = `translate(${o.left - n.left}px, ${o.top - n.top}px)`;
      requestAnimationFrame(() => {
        el.style.transition = "transform 200ms ease";
        el.style.transform = "";
      });
    }
    next.set(id, n);
  }
  rects.current = next;
}, [layoutKey]); // layoutKey = layout.map(w=>w.id+':'+w.span).join(',')
```

**Resize:** `onPointerDown` на ручке → `setPointerCapture`, `{id, x0, span0}`; `onPointerMove` → `span = clampSpan(id, span0 + Math.round(dx / colW))`, `colW = (gridRect.width - 3*gap) / 4`; live-preview в state (не onChange), `onPointerUp` → `onChange`.

Коммит: `feat(dash): editable widget grid (drag, resize, flip)`.

### Задача 4: интеграция в DashboardPage + i18n

**Изменения в `src/pages/DashboardPage.tsx`:**

1. `const [editing, setEditing] = useState(false);` + `const isLg = useMediaQuery("(min-width: 1024px)");` + `const { get, set } = useLocalSettings();` (импорт из `@/pages/settings/useLocalSettings`).
2. `visible: WidgetId[]` — memo: `["stats", ...(ratesOn?["rates"]:[]), ...(видимые chart_*), "accounts"]` (accounts — только если `accounts.length > 0`, иначе виджет пустой).
3. `layout = useMemo(() => normalizeLayout(get("dash_layout", ""), visible), [settings.data, visible])`.
4. `renderWidget(id)`: switch — `stats` → JSX трёх стат-карточек (обёртка `grid grid-cols-3 gap-3 h-full`, каждая мини-карточка `card`); `rates` → `<RatesCard ...>`; `chart_*` → соответствующая карточка диаграммы (выносим текущие блоки в функцию); `accounts` → `grid gap-3 sm:grid-cols-2 lg:grid-cols-3` с AccountCard.
   - Внимание: при span 2 блок stats должен сжиматься — внутренняя сетка stats: `grid-cols-1 @min-[560px]:grid-cols-3`? Проще: `grid grid-cols-3 gap-2` и пусть сжимается (min span 3 ≈ 650px — ок). Accounts при span 2: `lg:grid-cols-3` внутри 430px даст слишком узкие карточки — внутреннюю сетку оставить `grid-cols-2` при любом span? Решение: внутренние сетки stats и accounts делаем `grid-cols-[repeat(auto-fill,minmax(180px,1fr))]` — самоподстройка по ширине.
5. Два пути рендера:
   - `isLg` → `<DashGrid layout={layout} editing={editing} renderWidget widgetTitle onChange={(l)=>set("dash_layout", JSON.stringify(l))} />`
   - `!isLg` → текущий JSX без изменений (те же `renderWidget` вызовы в прежнем порядке — стат, курсы, диаграммы, счета).
   - Карандаш в заголовке (рядом с `RefreshButton`, иконка `Pencil`): виден только `isLg`; в edit mode на его месте кнопки `«Готово»` (Button size sm default) и `«Сбросить вид»` (variant outline, onClick → `set("dash_layout", JSON.stringify(defaultLayout(visible)))`).
   - Под заголовком в edit mode — `text-xs text-muted-foreground` с подсказкой `ui.editHint`.
6. `widgetTitle(id)` → `t(...)`: stats→`ui.totalBalance`… лучше отдельные имена: `settings.dashRates` для rates, `ui.pieMonth`/`incomeByCat`/… для диаграмм, новый `ui.widgetStats`/`ui.widgetAccounts` для stats/accounts.

**Новые ключи i18n** (в блок `ui` во всех трёх локалях, рядом с `journal`):

| Ключ | ru | be | en |
|---|---|---|---|
| `editLayout` | Изменить вид | Змяніць выгляд | Edit layout |
| `done` | Готово | Гатова | Done |
| `resetLayout` | Сбросить вид | Скінуць выгляд | Reset layout |
| `editHint` | Перетаскивайте карточки, за уголок меняйте ширину | Перацягвайце карткі, за вугал мяняйце шырыню | Drag cards around, pull the corner to change width |
| `moveEarlier` | Раньше | Раней | Move earlier |
| `moveLater` | Позже | Пазней | Move later |
| `wider` | Шире | Шырэй | Wider |
| `narrower` | Уже | Вужэй | Narrower |
| `widgetStats` | Сводка | Зводка | Summary |
| `widgetAccounts` | Счета | Рахункі | Accounts |

Коммит: `feat(dash): edit mode — drag/resize widgets, local layout`.

### Задача 5: документация, проверки, показать результат

- `CHANGELOG.md`: раздел `## Невыпущено` → подраздел «Дашборд»: режим редактирования, перетаскивание, ширина за уголок, кнопки-альтернативы, локальное хранение, только lg+.
- `src/lib/whatsnew.ts` НЕ трогаем (заполняется при следующем релизе).
- Полный прогон: `npx tsc --noEmit`, `npm run lint`, `npm test`, `npm run build`. Rust не тронут — cargo не нужен.
- Показать результат пользователю; релиз — только по команде.

## Review focus (на что смотреть при проверке)

1. Переполнение: chart при span 2 (~430px) — легенда не вылезает; rates при span 3 (~650px) — таблица влезает; stats при span 3 — три карточки читаемы.
2. Drag не стартует от микродвижения (порог 6px) и не ломает клик по кнопкам «‹ › − +» (pointer events на кнопках — stopPropagation).
3. После выключения диаграммы в настройках раскладка не падает (normalize выкидывает невидимые).
4. Узкое окно (<1024px): дефолтный вид, карандаша нет.
5. Сброс возвращает дефолт; битый `dash_layout` в БД не роняет страницу.
6. FLIP не дёргает призрак и не анимирует при первой отрисовке.
