import "./styles.css";
import { ManagementWorkbench } from "./workbench/management-workbench";

const app = document.querySelector<HTMLDivElement>("#app");

if (app === null) {
  throw new Error("application root is missing");
}

const shell = document.createElement("main");
shell.className = "shell";
const themeToggle = document.createElement("button");
themeToggle.id = "theme-toggle";
themeToggle.className = "theme-toggle";
themeToggle.type = "button";
const panel = document.createElement("section");
panel.id = "access-panel";
panel.className = "panel";
panel.setAttribute("aria-labelledby", "panel-title");
panel.setAttribute("aria-live", "polite");
const sharedViewHost = document.createElement("div");
sharedViewHost.id = "session-view-cache";
shell.append(themeToggle, panel, sharedViewHost);
app.replaceChildren(shell);
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
