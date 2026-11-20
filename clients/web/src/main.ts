import "./styles.css";
import { requiredElement } from "./workbench/dom-primitives";
import { ManagementWorkbench } from "./workbench/management-workbench";

const app = document.querySelector<HTMLDivElement>("#app");

if (app === null) {
  throw new Error("application root is missing");
}

app.innerHTML = `
  <main class="shell">
    <button id="theme-toggle" class="theme-toggle" type="button" aria-label="切换为深色主题">深色</button>
    <section
      id="access-panel"
      class="panel"
      aria-labelledby="panel-title"
      aria-live="polite"
    ></section>
    <div id="session-view-cache"></div>
  </main>
`;

const panel = requiredElement("access-panel", HTMLElement);
const sharedViewHost = requiredElement("session-view-cache", HTMLElement);
const themeToggle = requiredElement("theme-toggle", HTMLButtonElement);
const systemTheme = globalThis.matchMedia("(prefers-color-scheme: dark)");
let themeOverride: boolean | null = null;

function updateTheme(): void {
  const dark = themeOverride ?? systemTheme.matches;
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  themeToggle.textContent = dark ? "浅色" : "深色";
  themeToggle.setAttribute(
    "aria-label",
    dark ? "切换为浅色主题" : "切换为深色主题",
  );
}

themeToggle.addEventListener("click", () => {
  themeOverride = document.documentElement.dataset.theme !== "dark";
  updateTheme();
});
systemTheme.addEventListener("change", updateTheme);
updateTheme();

const workbench = new ManagementWorkbench({
  panel,
  sharedViewHost,
});

window.addEventListener("pagehide", () => {
  workbench.handlePageHide();
});
window.addEventListener("pageshow", (event) => {
  workbench.handlePageShow(event.persisted);
});
window.addEventListener("online", () => {
  workbench.handleOnline();
});

void workbench.boot();
