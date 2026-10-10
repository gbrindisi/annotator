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

/** Auto-grow a textarea to fit its content. */
export const autoGrow = (textarea: HTMLTextAreaElement): void => {
  textarea.style.height = 'auto';
  textarea.style.height = `${textarea.scrollHeight}px`;
};
