// Full-window zoom overlay for a rendered diagram: fit to screen, wheel/pinch zoom, drag to pan.
// Dependency-free. Esc, the close button or a click outside the diagram closes it.

const MIN_SCALE = 0.1;
const MAX_SCALE = 12;
const FIT_MARGIN = 0.92;
const DRAG_SLOP = 4;

/** Natural size of an SVG (width/height attributes, else viewBox). */
export const svgSize = (svg: SVGElement): { w: number; h: number } => {
  const num = (v: string | null): number => {
    const n = v && !v.endsWith('%') ? parseFloat(v) : NaN;
    return Number.isFinite(n) && n > 0 ? n : 0;
  };
  let w = num(svg.getAttribute('width'));
  let h = num(svg.getAttribute('height'));
  if (!w || !h) {
    const vb = (svg.getAttribute('viewBox') ?? '').trim().split(/[\s,]+/).map(Number);
    if (vb.length === 4 && vb[2] > 0 && vb[3] > 0) {
      w = w || vb[2];
      h = h || vb[3];
    }
  }
  return { w: w || 800, h: h || 600 };
};

export interface ZoomHandle {
  close(): void;
  readonly el: HTMLElement;
}

let open: ZoomHandle | null = null;

export const closeZoom = (): void => open?.close();

export const openZoom = (source: SVGElement, doc: Document = document): ZoomHandle => {
  closeZoom();
  const win = doc.defaultView ?? window;
  const svg = source.cloneNode(true) as SVGElement;
  const { w, h } = svgSize(source);
  svg.removeAttribute('style');
  svg.setAttribute('width', String(w));
  svg.setAttribute('height', String(h));
  svg.classList.add('mermaid-zoom-svg');

  const stage = doc.createElement('div');
  stage.className = 'mermaid-zoom-stage mmd';
  stage.append(svg);
  const hint = doc.createElement('div');
  hint.className = 'mermaid-zoom-hint';
  hint.textContent = 'Scroll to zoom · drag to pan · double-click to fit · Esc to close';
  const closeBtn = doc.createElement('button');
  closeBtn.type = 'button';
  closeBtn.className = 'mermaid-zoom-close';
  closeBtn.setAttribute('aria-label', 'Close');
  closeBtn.textContent = '×';
  const overlay = doc.createElement('div');
  overlay.className = 'mermaid-zoom';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  overlay.setAttribute('aria-label', 'Diagram');
  overlay.append(stage, hint, closeBtn);

  let scale = 1;
  let tx = 0;
  let ty = 0;
  const apply = () => {
    stage.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
  };
  const viewport = () => ({ vw: overlay.clientWidth || win.innerWidth, vh: overlay.clientHeight || win.innerHeight });
  const fit = () => {
    const { vw, vh } = viewport();
    scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.min((vw * FIT_MARGIN) / w, (vh * FIT_MARGIN) / h)));
    tx = (vw - w * scale) / 2;
    ty = (vh - h * scale) / 2;
    apply();
  };
  /** Zoom by `factor`, keeping the point (cx, cy) in viewport coordinates fixed. */
  const zoomAt = (factor: number, cx: number, cy: number) => {
    const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale * factor));
    const k = next / scale;
    tx = cx - (cx - tx) * k;
    ty = cy - (cy - ty) * k;
    scale = next;
    apply();
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    // Trackpad pinch arrives as ctrl+wheel with small deltas; mouse wheels as larger steps.
    const k = Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015));
    zoomAt(k, e.clientX, e.clientY);
  };

  const pointers = new Map<number, { x: number; y: number }>();
  let moved = 0;
  let pinch = 0;
  let downOnSvg = false;
  const dist = () => {
    const [a, b] = [...pointers.values()];
    return Math.hypot(a.x - b.x, a.y - b.y);
  };
  const onDown = (e: PointerEvent) => {
    if (e.target === closeBtn) return;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      moved = 0;
      downOnSvg = svg.contains(e.target as Node);
    }
    if (pointers.size === 2) pinch = dist();
    try {
      overlay.setPointerCapture(e.pointerId);
    } catch {
      /* not capturable (synthetic event) */
    }
    overlay.classList.add('is-dragging');
  };
  const onMove = (e: PointerEvent) => {
    const p = pointers.get(e.pointerId);
    if (!p) return;
    const dx = e.clientX - p.x;
    const dy = e.clientY - p.y;
    p.x = e.clientX;
    p.y = e.clientY;
    moved += Math.abs(dx) + Math.abs(dy);
    if (pointers.size === 2) {
      const d = dist();
      if (pinch > 0 && d > 0) {
        const [a, b] = [...pointers.values()];
        zoomAt(d / pinch, (a.x + b.x) / 2, (a.y + b.y) / 2);
      }
      pinch = d;
    } else if (pointers.size === 1) {
      tx += dx;
      ty += dy;
      apply();
    }
  };
  const onUp = (e: PointerEvent) => {
    if (!pointers.delete(e.pointerId)) return;
    pinch = 0;
    if (pointers.size === 0) overlay.classList.remove('is-dragging');
    // A click (no drag) on the backdrop, outside the diagram, closes.
    if (pointers.size === 0 && moved < DRAG_SLOP && e.type === 'pointerup' && !downOnSvg) handle.close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      handle.close();
    } else if (e.key === '+' || e.key === '=') zoomAt(1.25, viewport().vw / 2, viewport().vh / 2);
    else if (e.key === '-') zoomAt(0.8, viewport().vw / 2, viewport().vh / 2);
    else if (e.key === '0') fit();
  };
  const onResize = () => fit();

  overlay.addEventListener('wheel', onWheel, { passive: false });
  overlay.addEventListener('pointerdown', onDown);
  overlay.addEventListener('pointermove', onMove);
  overlay.addEventListener('pointerup', onUp);
  overlay.addEventListener('pointercancel', onUp);
  overlay.addEventListener('dblclick', e => {
    if (e.target !== closeBtn) fit();
  });
  closeBtn.addEventListener('click', () => handle.close());
  doc.addEventListener('keydown', onKey, true);
  win.addEventListener('resize', onResize);

  const handle: ZoomHandle = {
    el: overlay,
    close() {
      if (open !== handle) return;
      open = null;
      doc.removeEventListener('keydown', onKey, true);
      win.removeEventListener('resize', onResize);
      overlay.remove();
    },
  };
  doc.body.append(overlay);
  open = handle;
  fit();
  return handle;
};
