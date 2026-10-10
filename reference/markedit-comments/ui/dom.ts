type Attrs = Record<string, string | boolean | undefined> & { text?: string };

/** Tiny element builder. `text` sets textContent; other keys become attributes. */
export const el = <K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  children: Array<Node | string | null | undefined> = [],
): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    if (key === 'text') node.textContent = String(value);
    else if (key === 'className') node.className = String(value);
    else node.setAttribute(key, value === true ? '' : String(value));
  }
  for (const child of children) {
    if (child === null || child === undefined) continue;
    node.append(child);
  }
  return node;
};

/** Events that must never bubble out of the sidebar into the editor or MarkEdit's document handlers. */
const ISOLATED_EVENTS = ['mousedown', 'mouseup', 'click', 'dblclick', 'keydown', 'keyup', 'keypress', 'input', 'beforeinput', 'paste', 'cut', 'copy', 'contextmenu'];

export const isolateEvents = (root: HTMLElement): void => {
  for (const type of ISOLATED_EVENTS) {
    root.addEventListener(type, event => event.stopPropagation());
  }
};

/** Auto-grow a textarea to fit its content. */
export const autoGrow = (textarea: HTMLTextAreaElement): void => {
  textarea.style.height = 'auto';
  textarea.style.height = `${textarea.scrollHeight}px`;
};

export const appendStyle = (css: string): HTMLStyleElement => {
  const style = document.createElement('style');
  style.textContent = css;
  document.head.appendChild(style);
  return style;
};
