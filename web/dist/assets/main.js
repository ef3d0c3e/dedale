import { apiUrl } from "./url.js";
const token = new URLSearchParams(location.search).get("token") ?? "";
const status = document.querySelector("#status");
if (status) {
    status.textContent = `API at ${apiUrl("/api/session", token)}`;
}
