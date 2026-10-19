import { boot } from "./app.js";
const root = document.querySelector("#app");
if (root)
    void boot(root);
