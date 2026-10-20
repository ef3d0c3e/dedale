// A small helper to build elements, in place of a UI framework.

export type Child = Node | string | number | null | undefined | false | Child[];
export type Props = Record<string, string | number | boolean | null | undefined | EventListener>;

/** `<tag props...>children</tag>`; `on…` props are listeners, `false` and `null` are left out. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Props = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (typeof value === "function") {
      element.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (value === true) {
      element.setAttribute(key, "");
    } else if (value !== false && value !== null && value !== undefined) {
      element.setAttribute(key, String(value));
    }
  }
  append(element, children);
  return element;
}

export function append(parent: Node, children: Child[]): void {
  for (const child of children) {
    if (Array.isArray(child)) append(parent, child);
    else if (child instanceof Node) parent.appendChild(child);
    else if (child !== null && child !== undefined && child !== false) {
      parent.appendChild(document.createTextNode(String(child)));
    }
  }
}

/** Selects `value` when `select` offers it; whether it does. */
export function pick(select: HTMLSelectElement, value: string): boolean {
  const offered = [...select.options].some((option) => option.value === value);
  if (offered) select.value = value;
  return offered;
}

/** Replaces everything in `parent`. */
export function replace(parent: Node, ...children: Child[]): void {
  while (parent.firstChild) parent.removeChild(parent.firstChild);
  append(parent, children);
}
