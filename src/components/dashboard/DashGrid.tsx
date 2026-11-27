import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import ReactGridLayout, { useContainerWidth, verticalCompactor, type Layout, type LayoutItem } from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import "react-resizable/css/styles.css";
import {
  ChevronLeft, ChevronRight, FoldHorizontal, FoldVertical, GripVertical, UnfoldHorizontal, UnfoldVertical,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  clampH, clampW, flowToGrid, readingOrder, rowsFor, widgetDef,
  GRID_COLS, GRID_MARGIN, ROW_HEIGHT,
  type GridItem, type ParsedLayout, type WidgetId,
} from "@/lib/dashLayout";

const NO_DRAG = "dash-no-drag";

function toRgl(items: GridItem[]): Layout {
  return items.map((it) => ({
    ...it,
    minW: widgetDef(it.i).minW,
    minH: widgetDef(it.i).minH,
    maxW: GRID_COLS,
  }));
}

function fromRgl(layout: Layout): GridItem[] {
  return layout.map((l: LayoutItem) => ({ i: l.i as WidgetId, x: l.x, y: l.y, w: l.w, h: l.h }));
}

const sameItems = (a: GridItem[], b: GridItem[]) =>
  a.length === b.length &&
  a.every((x) => {
    const y = b.find((e) => e.i === x.i);
    return !!y && y.x === x.x && y.y === x.y && y.w === x.w && y.h === x.h;
  });

/**
 * Dashboard widget grid on react-grid-layout: 12 columns, 24px rows.
 * In edit mode cards are dragged by their body, snap to grid cells with a
 * placeholder preview, and resize from the bottom-right corner in both
 * directions; neighbours are pushed and compacted upward. A layout without
 * saved geometry ("flow") gets its heights measured from the real content,
 * so the stock dashboard has no empty space and no inner scrolling.
 */
export function DashGrid({
  layout,
  editing,
  disabled = false,
  renderWidget,
  widgetTitle,
  onCommit,
}: {
  layout: ParsedLayout;
  editing: boolean;
  disabled?: boolean;
  renderWidget: (id: WidgetId) => ReactNode;
  widgetTitle: (id: WidgetId) => string;
  onCommit: (items: GridItem[]) => void;
}) {
  const { t } = useTranslation();
  const { width, containerRef, mounted } = useContainerWidth();
  const colW = (width - GRID_MARGIN * (GRID_COLS - 1)) / GRID_COLS;
  const pxFor = (w: number) => Math.max(0, w * colW + (w - 1) * GRID_MARGIN);

  /* Auto-fit: measure natural content height of each widget at its width. */
  const [fit, setFit] = useState<Partial<Record<WidgetId, number>>>({});
  const measureRefs = useRef(new Map<WidgetId, HTMLElement>());
  const flow = layout.kind === "flow" ? layout.items : null;
  const flowKey = flow ? flow.map((f) => `${f.id}:${f.span}`).join(",") : "";

  useLayoutEffect(() => {
    if (!flow) return;
    const read = () => {
      const next: Partial<Record<WidgetId, number>> = {};
      for (const [id, el] of measureRefs.current) next[id] = rowsFor(el.offsetHeight);
      setFit((prev) => {
        const keys = Object.keys(next) as WidgetId[];
        return keys.length === Object.keys(prev).length && keys.every((k) => prev[k] === next[k]) ? prev : next;
      });
    };
    read();
    const ro = new ResizeObserver(read);
    for (const el of measureRefs.current.values()) ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flowKey, width]);

  const items = useMemo<GridItem[]>(
    () => (layout.kind === "grid" ? layout.items : flowToGrid(layout.items, fit)),
    [layout, fit],
  );
  const rglLayout = useMemo(() => toRgl(items), [items]);

  /*
   * react-grid-layout 2.3.0 memoizes GridItem on x/y/w/h and `children`, but
   * keeps the live drag/resize pixel position in a ref. Without a fresh child
   * element per pointer move the dragged card freezes and then jumps a whole
   * cell. Bumping `tick` re-creates only the thin item wrapper; the widget
   * bodies (`inner`) are memoized so heavy charts do not re-render.
   */
  const [, setTick] = useState(0);
  const tick = () => setTick((n) => (n + 1) % 1_000_000);

  const inner = useMemo(() => {
    const commit = (next: GridItem[]) => {
      const compacted = fromRgl(verticalCompactor.compact(toRgl(next), GRID_COLS));
      if (!sameItems(compacted, items)) onCommit(compacted);
    };
    const order = readingOrder(items);
    const reorder = (id: WidgetId, dir: -1 | 1) => {
      const i = order.findIndex((e) => e.i === id);
      const j = i + dir;
      if (i < 0 || j < 0 || j >= order.length) return;
      const a = order[i], b = order[j];
      const ax = Math.min(b.x, GRID_COLS - a.w), bx = Math.min(a.x, GRID_COLS - b.w);
      commit(items.map((e) =>
        e.i === a.i ? { ...e, x: ax, y: b.y } : e.i === b.i ? { ...e, x: bx, y: a.y } : e,
      ));
    };
    const resizeBy = (id: WidgetId, dw: number, dh: number) => {
      commit(items.map((e) => {
        if (e.i !== id) return e;
        const w = clampW(id, e.w + dw);
        return { ...e, w, x: Math.min(e.x, GRID_COLS - w), h: clampH(id, e.h + dh) };
      }));
    };
    const btn =
      "grid h-6 w-6 place-items-center rounded-md border border-border bg-card text-muted-foreground shadow-sm transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary disabled:opacity-30";
    const tool = (Icon: LucideIcon, label: string, onClick: () => void, off: boolean) => (
      <button type="button" className={btn} disabled={disabled || off} aria-label={label} title={label} onClick={onClick}>
        <Icon className="h-3.5 w-3.5" />
      </button>
    );

    const map = new Map<WidgetId, ReactNode>();
    for (const it of items) {
      const idx = order.findIndex((e) => e.i === it.i);
      const d = widgetDef(it.i);
      const name = widgetTitle(it.i);
      map.set(it.i, (
        <>
          <div
            className={cn(
              "dash-scroll h-full overflow-auto rounded-2xl [&>*]:min-h-full",
              editing && "pointer-events-none select-none",
            )}
          >
            {renderWidget(it.i)}
          </div>
          {editing && (
            <>
              <div className="pointer-events-none absolute inset-0 rounded-2xl outline-2 outline-dashed outline-primary/50 -outline-offset-2" />
              <div
                className={cn(
                  NO_DRAG,
                  "absolute right-8 top-1.5 z-10 flex gap-0.5 rounded-lg bg-card/95 p-0.5 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100",
                )}
              >
                {tool(ChevronLeft, `${t("ui.moveEarlier")} — ${name}`, () => reorder(it.i, -1), idx <= 0)}
                {tool(ChevronRight, `${t("ui.moveLater")} — ${name}`, () => reorder(it.i, 1), idx >= order.length - 1)}
                {tool(FoldHorizontal, `${t("ui.narrower")} — ${name}`, () => resizeBy(it.i, -1, 0), it.w <= d.minW)}
                {tool(UnfoldHorizontal, `${t("ui.wider")} — ${name}`, () => resizeBy(it.i, 1, 0), it.w >= GRID_COLS)}
                {tool(FoldVertical, `${t("ui.shorter")} — ${name}`, () => resizeBy(it.i, 0, -1), it.h <= d.minH)}
                {tool(UnfoldVertical, `${t("ui.taller")} — ${name}`, () => resizeBy(it.i, 0, 1), false)}
              </div>
              <GripVertical className="pointer-events-none absolute right-2 top-2 h-4 w-4 text-muted-foreground" />
            </>
          )}
        </>
      ));
    }
    return { map, commit };
  }, [items, editing, disabled, renderWidget, widgetTitle, onCommit, t]);

  return (
    <div ref={containerRef} className={cn("dash-grid relative", editing && "dash-grid-editing")}>
      {mounted && (
        <ReactGridLayout
          width={width}
          layout={rglLayout}
          gridConfig={{ cols: GRID_COLS, rowHeight: ROW_HEIGHT, margin: [GRID_MARGIN, GRID_MARGIN], containerPadding: [0, 0] }}
          dragConfig={{ enabled: editing && !disabled, cancel: `.${NO_DRAG}`, threshold: 4 }}
          resizeConfig={{
            enabled: editing && !disabled,
            handles: ["se"],
            handleComponent: (axis, ref) => (
              <span
                ref={ref as React.Ref<HTMLSpanElement>}
                className={`react-resizable-handle react-resizable-handle-${axis} dash-resize-handle`}
                aria-hidden="true"
              >
                <svg viewBox="0 0 12 12" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
                  <path d="M3 11 L11 3" />
                  <path d="M7 11 L11 7" />
                </svg>
              </span>
            ),
          }}
          compactor={verticalCompactor}
          onDrag={tick}
          onResize={tick}
          onDragStop={(l) => inner.commit(fromRgl(l))}
          onResizeStop={(l) => inner.commit(fromRgl(l))}
        >
          {items.map((it) => (
            <div key={it.i} className={cn("dash-item group min-w-0", editing && "cursor-grab active:cursor-grabbing")}>
              {inner.map.get(it.i)}
            </div>
          ))}
        </ReactGridLayout>
      )}
      {mounted && flow && (
        <div aria-hidden="true" inert className="pointer-events-none invisible absolute left-0 top-0 -z-10 overflow-hidden" style={{ height: 0 }}>
          {flow.map((f) => (
            <div
              key={f.id}
              ref={(el) => {
                if (el) measureRefs.current.set(f.id, el);
                else measureRefs.current.delete(f.id);
              }}
              style={{ width: pxFor(f.span) }}
            >
              {renderWidget(f.id)}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
