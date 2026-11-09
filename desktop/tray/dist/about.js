// SPDX-License-Identifier: GPL-3.0-or-later
// The About window: what the tray's app_info command says (desktop/tray/src/about.rs).
"use strict";

document.addEventListener("DOMContentLoaded", () => {
  const source = document.getElementById("source");
  window.pastegetCopyable(source, document.getElementById("copy_source"));
  document.getElementById("close").addEventListener("click", () => {
    const tauri = window.__TAURI__;
    if (tauri) tauri.core.invoke("close_window").catch(() => {});
  });
  window.pastegetAppInfo().then((info) => {
    if (!info) return;
    document.getElementById("version").textContent = info.version;
    source.value = info.source;
  });
});
