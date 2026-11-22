import { useLayoutEffect, useRef, useState, type ReactNode, type PointerEvent as RPointerEvent } from "react";
import { useTranslation } from "react-i18next";
import { GripVertical, Minus, MoveLeft, MoveRight, Plus } from "lucide-react";
import { cn } from "@/lib/utils";
import { clampSpan, GRID_COLS, WIDGET_DEFS, type WidgetId, type WidgetLayout } from "@/lib/dashLayout";

const DRAG_THRESHOLD = 6;
const GAP = 16; // gap-4

interface DragState {
  id: WidgetId;
  offX: number;
  offY: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Editable 4-column widget grid for the dashboard. In `editing` mode every
 * widget can be dragged (ghost follows the pointer, siblings reflow live)
 * and resized by a corner handle snapping to whole columns. Layout changes
 * are reported through `onChange` once a gesture completes; move/shrink/grow
 * buttons provide a non-drag alternative.
 */
export function DashGrid({
  layout,
  editing,
  renderWidget,
  widgetTitle,
  onChange,
}: {
  layout: WidgetLayout[];
  editing: boolean;
  renderWidget: (id: WidgetId) => ReactNode;
  widgetTitle: (id: WidgetId) => string;
  onChange: (l: WidgetLayout[]) => void;
}) {
  const { t } = useTranslation();
  const [drag, setDrag] = useState<DragState | null>(null);
  const [order, setOrder] = useState<WidgetLayout[] | null>(null);
  const [resize, setResize] = useState<{ id: WidgetId; span: number } | null>(null);

  const items = order ?? layout.map((w) => (resize?.id === w.id ? { ...w, span: resize.span } : w));
  const layoutKey = items.map((w) => `${w.id}:${w.span}`).join(",");
  const itemRefs = useRef(new Map<WidgetId, HTMLElement>());
  const rects = useRef(new Map<WidgetId, DOMRect>());
  const gridRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ id: WidgetId; startX: number; startY: number; el: HTMLElement } | null>(null);
  const resizeRef = useRef<{ id: WidgetId; x0: number; span0: number; colW: number } | null>(null);

  /* FLIP: animate siblings to their new positions after a reflow. */
  useLayoutEffect(() => {
    const next = new Map<WidgetId, DOMRect>();
    for (const [id, el] of itemRefs.current) {
      const n = el.getBoundingClientRect();
      const o = rects.current.get(id);
      if (o && drag?.id !== id && (o.left !== n.left || o.top !== n.top)) {
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
  }, [layoutKey, drag]);

  const onWidgetPointerDown = (e: RPointerEvent<HTMLDivElement>, id: WidgetId) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    const el = itemRefs.current.get(id);
    if (!el) return;
    dragRef.current = { id, startX: e.clientX, startY: e.clientY, el };
    el.setPointerCapture(e.pointerId);
  };

  const onWidgetPointerMove = (e: RPointerEvent<HTMLDivElement>, id: WidgetId) => {
    const d = dragRef.current;
    if (!d || d.id !== id) return;
    if (!drag && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return;
    if (!drag) {
      const r = d.el.getBoundingClientRect();
      setDrag({ id, offX: d.startX - r.left, offY: d.startY - r.top, x: e.clientX, y: e.clientY, w: r.width, h: r.height });
      setOrder(layout.slice());
      return;
    }
    setDrag({ ...drag, x: e.clientX, y: e.clientY });
    const cur = order ?? layout;
    let idx = 0;
    for (const w of cur) {
      if (w.id === id) continue;
      const el = itemRefs.current.get(w.id);
      if (!el) continue;
      const r = el.getBoundingClientRect();
      const cy = r.top + r.height / 2;
      const cx = r.left + r.width / 2;
      if (e.clientY > cy || (Math.abs(e.clientY - cy) < r.height / 2 && e.clientX > cx)) idx++;
    }
    const without = cur.filter((w) => w.id !== id);
    const item = cur.find((w) => w.id === id)!;
    const next = [...without.slice(0, idx), item, ...without.slice(idx)];
    if (next.map((w) => w.id).join() !== cur.map((w) => w.id).join()) setOrder(next);
  };

  const endDrag = (id: WidgetId) => {
    const d = dragRef.current;
    dragRef.current = null;
    if (d?.id === id && drag && order) onChange(order);
    setDrag(null);
    setOrder(null);
  };

  const onHandleDown = (e: RPointerEvent<HTMLButtonElement>, id: WidgetId) => {
    e.stopPropagation();
    const grid = gridRef.current?.getBoundingClientRect();
    if (!grid) return;
    const colW = (grid.width - (GRID_COLS - 1) * GAP) / GRID_COLS;
    const span0 = layout.find((w) => w.id === id)?.span ?? 1;
    resizeRef.current = { id, x0: e.clientX, span0, colW };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onHandleMove = (e: RPointerEvent<HTMLButtonElement>, id: WidgetId) => {
    const r = resizeRef.current;
    if (!r || r.id !== id) return;
    const span = clampSpan(id, r.span0 + Math.round((e.clientX - r.x0) / (r.colW + GAP)));
    setResize((prev) => (prev?.id === id && prev.span === span ? prev : { id, span }));
  };

  const endResize = (id: WidgetId) => {
    const r = resizeRef.current;
    resizeRef.current = null;
    if (r && resize) onChange(layout.map((w) => (w.id === id ? { ...w, span: resize.span } : w)));
    setResize(null);
  };

  const move = (id: WidgetId, dir: -1 | 1) => {
    const i = layout.findIndex((w) => w.id === id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= layout.length) return;
    const next = layout.slice();
    [next[i], next[j]] = [next[j], next[i]];
    onChange(next);
  };

  const bump = (id: WidgetId, d: -1 | 1) => {
    const w = layout.find((x) => x.id === id);
    if (!w) return;
    const span = clampSpan(id, w.span + d);
    if (span !== w.span) onChange(layout.map((x) => (x.id === id ? { ...x, span } : x)));
  };

  const btn =
    "grid h-6 w-6 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30";

  return (
    <div ref={gridRef} className="grid grid-cols-4 items-start gap-4">
      {items.map((w) => {
        const isDragged = drag?.id === w.id;
        const i = layout.findIndex((x) => x.id === w.id);
        const def = WIDGET_DEFS[w.id];
        return (
          <div
            key={w.id}
            ref={(el) => {
              if (el) itemRefs.current.set(w.id, el);
              else itemRefs.current.delete(w.id);
            }}
            style={{ gridColumn: `span ${w.span} / span ${w.span}` }}
            className={cn("relative min-w-0", editing && "touch-none")}
            onPointerDown={editing ? (e) => onWidgetPointerDown(e, w.id) : undefined}
            onPointerMove={editing ? (e) => onWidgetPointerMove(e, w.id) : undefined}
            onPointerUp={editing ? () => endDrag(w.id) : undefined}
            onPointerCancel={editing ? () => endDrag(w.id) : undefined}
          >
            {isDragged ? (
              <div className="rounded-2xl border-2 border-dashed border-primary/50" style={{ height: drag.h }} />
            ) : (
              <>
                <div
                  className={cn(
                    editing &&
                      "pointer-events-none select-none rounded-2xl outline-2 outline-dashed outline-primary/40 -outline-offset-4",
                  )}
                >
                  {renderWidget(w.id)}
                </div>
                {editing && (
                  <>
                    <div className="absolute left-2 top-2 z-10 flex gap-0.5">
                      <button type="button" className={btn} disabled={i <= 0}
                        aria-label={`${t("ui.moveEarlier")} — ${widgetTitle(w.id)}`}
                        onClick={() => move(w.id, -1)}>
                        <MoveLeft className="h-3.5 w-3.5" />
                      </button>
                      <button type="button" className={btn} disabled={i >= layout.length - 1}
                        aria-label={`${t("ui.moveLater")} — ${widgetTitle(w.id)}`}
                        onClick={() => move(w.id, 1)}>
                        <MoveRight className="h-3.5 w-3.5" />
                      </button>
                      <button type="button" className={btn} disabled={w.span <= def.min}
                        aria-label={`${t("ui.narrower")} — ${widgetTitle(w.id)}`}
                        onClick={() => bump(w.id, -1)}>
                        <Minus className="h-3.5 w-3.5" />
                      </button>
                      <button type="button" className={btn} disabled={w.span >= GRID_COLS}
                        aria-label={`${t("ui.wider")} — ${widgetTitle(w.id)}`}
                        onClick={() => bump(w.id, 1)}>
                        <Plus className="h-3.5 w-3.5" />
                      </button>
                    </div>
                    <GripVertical className="pointer-events-none absolute right-2 top-2 h-4 w-4 text-muted-foreground" />
                    <button
                      type="button"
                      aria-label={`${t("ui.wider")} / ${t("ui.narrower")} — ${widgetTitle(w.id)}`}
                      className="group absolute -bottom-1 -right-1 z-10 grid h-5 w-5 cursor-nwse-resize place-items-center text-primary/70 transition-colors hover:text-primary"
                      onPointerDown={(e) => onHandleDown(e, w.id)}
                      onPointerMove={(e) => onHandleMove(e, w.id)}
                      onPointerUp={() => endResize(w.id)}
                      onPointerCancel={() => endResize(w.id)}
                    >
                      <svg viewBox="0 0 12 12" className="h-3 w-3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
                        <path d="M2 10 L10 2" />
                        <path d="M6 10 L10 6" />
                      </svg>
                    </button>
                  </>
                )}
              </>
            )}
          </div>
        );
      })}
      {drag && (
        <div
          className="pointer-events-none fixed z-50 rounded-2xl opacity-80 shadow-2xl"
          style={{ left: drag.x - drag.offX, top: drag.y - drag.offY, width: drag.w, height: drag.h }}
        >
          {renderWidget(drag.id)}
        </div>
      )}
    </div>
  );
}
