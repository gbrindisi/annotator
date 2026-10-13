// Custom Mermaid look: `theme: 'base'` with explicit variables, tuned for legibility.
// Loaded lazily together with mermaid itself (see mermaid.ts). Companion CSS: style.css
// ("Mermaid diagrams"); a few things (arrowheads, edge-label backgrounds, cluster borders)
// cannot be reached through themeVariables.
//
// Contrast (WCAG AA, 4.5:1 for text) of every text/fill pair was computed when choosing these.

export type Scheme = 'light' | 'dark';

export const FONT_FAMILY = "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Inter', system-ui, sans-serif";

interface Palette {
  dark: boolean;
  /** Diagram card = the page behind labels (edge-label backgrounds match it). */
  card: string;
  text: string;
  /** Edges / arrows. */
  line: string;
  node: { fill: string; border: string; text: string };
  second: { fill: string; border: string; text: string };
  third: { fill: string; border: string; text: string };
  cluster: { fill: string; border: string; title: string };
  note: { fill: string; border: string; text: string };
  activation: { fill: string; border: string };
  /** Pie slices (labels sit on them in `pieText`). */
  pie: string[];
  pieText: string;
  /** Categorical fills for journey/timeline/mindmap sections, with a readable label colour. */
  cat: string[];
  catText: string;
  grid: string;
  gantt: { task: [string, string]; active: [string, string]; done: [string, string]; crit: [string, string]; today: string };
}

// Okabe-Ito based (colour-blind friendly), darkened so white labels pass AA on the slices.
const LIGHT: Palette = {
  dark: false,
  card: '#f8f9fd',
  text: '#0f172a',
  line: '#334155',
  node: { fill: '#e0e7ff', border: '#4338ca', text: '#0f172a' },
  second: { fill: '#dcfce7', border: '#15803d', text: '#052e16' },
  third: { fill: '#fef3c7', border: '#b45309', text: '#451a03' },
  cluster: { fill: '#eef1fb', border: '#6b7a99', title: '#1e293b' },
  note: { fill: '#fef3c7', border: '#d97706', text: '#451a03' },
  activation: { fill: '#c7d2fe', border: '#4338ca' },
  pie: ['#1d5fb4', '#c2410c', '#0f766e', '#a8327d', '#8a5a00', '#4b5563', '#6d28d9', '#0369a1', '#b91c1c', '#4d7c0f', '#9d174d', '#155e75'],
  pieText: '#ffffff',
  cat: ['#cfe3fb', '#fde0c8', '#c5ede4', '#f5d3e6', '#f3e3a6', '#dfe3ea', '#e1d7fa', '#c8e8f7', '#fbd0d0', '#dcefc2', '#f9d0e2', '#c3e9ee'],
  catText: '#0f172a',
  grid: '#cbd5e1',
  gantt: {
    task: ['#c7d2fe', '#4338ca'],
    active: ['#bae6fd', '#0369a1'],
    done: ['#d1fae5', '#047857'],
    crit: ['#fecaca', '#b91c1c'],
    today: '#dc2626',
  },
};

const DARK: Palette = {
  dark: true,
  card: '#1b1d23',
  text: '#f1f5f9',
  line: '#cbd5e1',
  node: { fill: '#262b5e', border: '#a5b4fc', text: '#f1f5f9' },
  second: { fill: '#14432b', border: '#86efac', text: '#ecfdf5' },
  third: { fill: '#4a3410', border: '#fcd34d', text: '#fffbeb' },
  cluster: { fill: '#22252d', border: '#7c8aa5', title: '#e2e8f0' },
  note: { fill: '#4a3a12', border: '#fbbf24', text: '#fef3c7' },
  activation: { fill: '#3f4590', border: '#a5b4fc' },
  pie: ['#56b4e9', '#ff9a4d', '#2dd4a7', '#e79ac4', '#f0e442', '#9ca3af', '#a78bfa', '#7dd3fc', '#f87171', '#a3e635', '#f9a8d4', '#67e8f9'],
  pieText: '#0b0d10',
  cat: ['#1e3a5f', '#5c2e0e', '#0f4a42', '#5a1f43', '#54450a', '#374151', '#3b2a73', '#0c4a6e', '#5f1d1d', '#2f4a10', '#5b1e3b', '#124a55'],
  catText: '#f1f5f9',
  grid: '#3b4252',
  gantt: {
    task: ['#3f4590', '#a5b4fc'],
    active: ['#0c4a6e', '#7dd3fc'],
    done: ['#14532d', '#86efac'],
    crit: ['#7f1d1d', '#fca5a5'],
    today: '#f87171',
  },
};

export const palette = (scheme: Scheme): Palette => (scheme === 'dark' ? DARK : LIGHT);

export const themeVariables = (scheme: Scheme): Record<string, string | boolean | number> => {
  const p = palette(scheme);
  const v: Record<string, string | boolean | number> = {
    darkMode: p.dark,
    background: p.card,
    fontFamily: FONT_FAMILY,
    fontSize: '16px',
    strokeWidth: 2,
    useGradient: false,
    dropShadow: 'none',
    textColor: p.text,
    titleColor: p.text,
    lineColor: p.line,
    arrowheadColor: p.line,
    defaultLinkColor: p.line,
    // Flowchart / generic nodes
    primaryColor: p.node.fill,
    primaryBorderColor: p.node.border,
    primaryTextColor: p.node.text,
    secondaryColor: p.second.fill,
    secondaryBorderColor: p.second.border,
    secondaryTextColor: p.second.text,
    tertiaryColor: p.third.fill,
    tertiaryBorderColor: p.third.border,
    tertiaryTextColor: p.third.text,
    mainBkg: p.node.fill,
    nodeBkg: p.node.fill,
    nodeBorder: p.node.border,
    nodeTextColor: p.node.text,
    border1: p.node.border,
    border2: p.second.border,
    clusterBkg: p.cluster.fill,
    clusterBorder: p.cluster.border,
    edgeLabelBackground: p.card,
    labelBackground: p.card,
    // Sequence
    actorBkg: p.node.fill,
    actorBorder: p.node.border,
    actorTextColor: p.node.text,
    actorLineColor: p.dark ? '#94a3b8' : '#64748b',
    signalColor: p.line,
    signalTextColor: p.text,
    labelBoxBkgColor: p.node.fill,
    labelBoxBorderColor: p.node.border,
    labelTextColor: p.node.text,
    loopTextColor: p.text,
    noteBkgColor: p.note.fill,
    noteBorderColor: p.note.border,
    noteTextColor: p.note.text,
    activationBkgColor: p.activation.fill,
    activationBorderColor: p.activation.border,
    sequenceNumberColor: p.dark ? '#0b0d10' : '#ffffff',
    rectBkgColor: p.cluster.fill,
    // Class / state / ER
    classText: p.node.text,
    stateBkg: p.node.fill,
    stateLabelColor: p.node.text,
    labelColor: p.text,
    labelBackgroundColor: p.card,
    transitionColor: p.line,
    transitionLabelColor: p.text,
    specialStateColor: p.line,
    compositeBackground: p.cluster.fill,
    compositeTitleBackground: p.node.fill,
    compositeBorder: p.cluster.border,
    altBackground: p.cluster.fill,
    innerEndBackground: p.node.border,
    attributeBackgroundColorOdd: p.dark ? '#22252d' : '#ffffff',
    attributeBackgroundColorEven: p.dark ? '#2a2e38' : '#eef1fb',
    relationColor: p.line,
    relationLabelColor: p.text,
    relationLabelBackground: p.card,
    // Pie
    pieTitleTextSize: '22px',
    pieTitleTextColor: p.text,
    pieSectionTextSize: '15px',
    pieSectionTextColor: p.pieText,
    pieLegendTextSize: '15px',
    pieLegendTextColor: p.text,
    pieStrokeColor: p.card,
    pieStrokeWidth: '2px',
    pieOuterStrokeColor: p.card,
    pieOuterStrokeWidth: '2px',
    pieOpacity: '1',
    // Gantt
    gridColor: p.grid,
    sectionBkgColor: p.dark ? '#22252d' : '#eef1fb',
    altSectionBkgColor: p.card,
    sectionBkgColor2: p.dark ? '#2a2e38' : '#e3e8f8',
    excludeBkgColor: p.dark ? '#2a2e38' : '#e8eaf0',
    taskBkgColor: p.gantt.task[0],
    taskBorderColor: p.gantt.task[1],
    activeTaskBkgColor: p.gantt.active[0],
    activeTaskBorderColor: p.gantt.active[1],
    doneTaskBkgColor: p.gantt.done[0],
    doneTaskBorderColor: p.gantt.done[1],
    critBkgColor: p.gantt.crit[0],
    critBorderColor: p.gantt.crit[1],
    todayLineColor: p.gantt.today,
    vertLineColor: p.gantt.today,
    taskTextColor: p.text,
    taskTextLightColor: p.text,
    taskTextDarkColor: p.text,
    taskTextOutsideColor: p.text,
    taskTextClickableColor: p.dark ? '#7dd3fc' : '#1d4ed8',
  };
  p.cat.forEach((c, i) => {
    v[`cScale${i}`] = c;
    v[`cScaleLabel${i}`] = p.catText;
    v[`cScalePeer${i}`] = p.dark ? '#a5b4fc' : '#4338ca';
  });
  p.pie.forEach((c, i) => (v[`pie${i + 1}`] = c));
  // Journey sections
  p.cat.slice(0, 8).forEach((c, i) => (v[`fillType${i}`] = c));
  return v;
};

const NO_MAX_WIDTH = [
  'flowchart', 'sequence', 'gantt', 'journey', 'class', 'state', 'er', 'pie', 'requirement', 'mindmap',
  'timeline', 'gitGraph', 'c4', 'quadrantChart', 'xyChart', 'sankey', 'block', 'architecture', 'packet', 'kanban', 'radar',
] as const;

/** Full mermaid.initialize() config. */
export const mermaidConfig = (scheme: Scheme): Record<string, unknown> => {
  const cfg: Record<string, unknown> = {
    startOnLoad: false,
    securityLevel: 'strict',
    theme: 'base',
    themeVariables: themeVariables(scheme),
    fontFamily: FONT_FAMILY,
    fontSize: 16,
    suppressErrorRendering: true,
    logLevel: 'fatal',
  };
  // Natural size (no shrink-to-fit): the container scrolls, the zoom overlay fits to screen.
  for (const k of NO_MAX_WIDTH) cfg[k] = { useMaxWidth: false };
  Object.assign(cfg, {
    flowchart: { useMaxWidth: false, htmlLabels: true, curve: 'basis', padding: 14, nodeSpacing: 50, rankSpacing: 56, diagramPadding: 12 },
    sequence: {
      useMaxWidth: false, actorMargin: 56, width: 160, height: 48, boxMargin: 10, boxTextMargin: 6, noteMargin: 12, messageMargin: 40,
      diagramMarginX: 24, diagramMarginY: 16, actorFontSize: 16, noteFontSize: 15, messageFontSize: 15, messageAlign: 'center',
    },
    gantt: { useMaxWidth: false, fontSize: 14, sectionFontSize: 15, barHeight: 26, barGap: 6, topPadding: 56, leftPadding: 96, useWidth: 880 },
    er: { useMaxWidth: false, fontSize: 15, entityPadding: 18 },
    pie: { useMaxWidth: false, textPosition: 0.72 },
    journey: { useMaxWidth: false, taskFontSize: 14, diagramMarginX: 40, diagramMarginY: 20 },
    state: { useMaxWidth: false, padding: 12 },
    class: { useMaxWidth: false, padding: 12 },
  });
  return cfg;
};
