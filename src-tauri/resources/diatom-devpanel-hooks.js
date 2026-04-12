
(function installDevPanelHooks() {
  "use strict";

  if (window.__DIATOM_DEVPANEL_HOOKS__) return;
  window.__DIATOM_DEVPANEL_HOOKS__ = true;

  let panelOpen = false;
  window.__diatom_devpanel_set_open = (v) => { panelOpen = v; };

  const LEVELS = ["log", "warn", "error", "info", "debug"];
  const _orig  = {};

  LEVELS.forEach((level) => {
    _orig[level] = console[level].bind(console);
    console[level] = (...args) => {
      _orig[level](...args);
      if (!panelOpen) return;

      const text = args
        .map((a) => typeof a === "object" ? JSON.stringify(a, null, 2) : String(a))
        .join(" ");

      let sourceFile = null;
      let sourceLine = null;
      try {
        const stack = new Error().stack || "";
        const m = (stack.split("\n")[2] || "").match(/(https?:\/\/[^:]+):(\d+)/);
        if (m) { sourceFile = m[1]; sourceLine = parseInt(m[2], 10); }
      } catch (_) {}

      window.__TAURI__.invoke("dev_panel_console_entry", {
        level, text, sourceFile, sourceLine,
      }).catch(() => {});
    };
  });

  window.addEventListener("error", (ev) => {
    if (!panelOpen) return;
    window.__TAURI__.invoke("dev_panel_console_entry", {
      level:      "error",
      text:       `Uncaught ${ev.message}`,
      sourceFile: ev.filename || null,
      sourceLine: ev.lineno  || null,
    }).catch(() => {});
  });

  window.addEventListener("unhandledrejection", (ev) => {
    if (!panelOpen) return;
    const text = ev.reason
      ? (ev.reason.stack || String(ev.reason))
      : "Unhandled promise rejection";
    window.__TAURI__.invoke("dev_panel_console_entry", {
      level: "error", text, sourceFile: null, sourceLine: null,
    }).catch(() => {});
  });

  function notifyNavigated() {
    if (!panelOpen) return;
    window.__TAURI__.invoke("dev_panel_navigate", {
      url:   location.href,
      title: document.title || location.hostname,
    }).catch(() => {});
  }

  document.addEventListener("DOMContentLoaded", notifyNavigated);
  window.addEventListener("popstate",   notifyNavigated);
  window.addEventListener("hashchange", notifyNavigated);

  document.addEventListener("keydown", (ev) => {
    const isF12      = ev.key === "F12";
    const isMacCombo = ev.key === "i" && ev.metaKey  && ev.altKey;   // Cmd+Opt+I
    const isWinCombo = ev.key === "I" && ev.ctrlKey  && ev.shiftKey; // Ctrl+Shift+I

    if (isF12 || isMacCombo || isWinCombo) {
      ev.preventDefault();
      window.__TAURI__
        .invoke("dev_panel_open", { projectRoot: null })
        .then(() => { panelOpen = true; })
        .catch(() => {});
    }
  });

  window.__diatom_open_in_zed = function openInZed(sourceUrl, line) {
    window.__TAURI__
      .invoke("dev_panel_open_in_zed", {
        url:  sourceUrl || location.href,
        line: line      || null,
      })
      .catch((err) => console.warn("[diatom] open-in-zed failed:", err));
  };

  window.__diatom_inject_open_in_zed_button = function(toolbar, sourceUrl, line) {
    if (!toolbar || toolbar.querySelector("[data-diatom-zed-btn]")) return;

    const btn = document.createElement("button");
    btn.setAttribute("data-diatom-zed-btn", "1");
    btn.title = "Open source file in external Zed IDE";
    btn.textContent = "Open in Zed";
    btn.style.cssText = [
      "font-size:11px",
      "padding:2px 7px",
      "border-radius:4px",
      "border:1px solid var(--diatom-border, #444)",
      "background:var(--diatom-surface2, #2a2a2a)",
      "color:var(--diatom-text, #e0e0e0)",
      "cursor:pointer",
      "margin-left:8px",
    ].join(";");

    btn.addEventListener("click", () => window.__diatom_open_in_zed(sourceUrl, line));
    toolbar.appendChild(btn);
  };

  window.__diatom_perf = {
    navigation: () => performance.getEntriesByType("navigation")[0] || null,
    paints:     () => performance.getEntriesByType("paint"),
    resources:  () => performance.getEntriesByType("resource").map((r) => ({
      name:         r.name,
      duration:     Math.round(r.duration),
      transferSize: r.transferSize,
    })),
  };
})();

