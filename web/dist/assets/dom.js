// A small helper to build elements, in place of a UI framework.
/** `<tag props...>children</tag>`; `on…` props are listeners, `false` and `null` are left out. */
export function h(tag, props = {}, ...children) {
    const element = document.createElement(tag);
    for (const [key, value] of Object.entries(props)) {
        if (typeof value === "function") {
            element.addEventListener(key.slice(2).toLowerCase(), value);
        }
        else if (value === true) {
            element.setAttribute(key, "");
        }
        else if (value !== false && value !== null && value !== undefined) {
            element.setAttribute(key, String(value));
        }
    }
    append(element, children);
    return element;
}
export function append(parent, children) {
    for (const child of children) {
        if (Array.isArray(child))
            append(parent, child);
        else if (child instanceof Node)
            parent.appendChild(child);
        else if (child !== null && child !== undefined && child !== false) {
            parent.appendChild(document.createTextNode(String(child)));
        }
    }
}
/** Replaces everything in `parent`. */
export function replace(parent, ...children) {
    while (parent.firstChild)
        parent.removeChild(parent.firstChild);
    append(parent, children);
}
