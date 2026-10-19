import { boot } from "./app.ts";

const root = document.querySelector<HTMLElement>("#app");
if (root) void boot(root);
