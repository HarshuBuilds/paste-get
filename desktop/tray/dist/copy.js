// SPDX-License-Identifier: GPL-3.0-or-later
// Text the user may want to copy (an address, a config line): a read-only field selected in one
// click, and a Copy button. Nothing here opens anything: paste-get makes no network calls.
"use strict";

// The tray's app_info command (desktop/tray/src/about.rs), or null outside paste-get.
window.pastegetAppInfo = () => {
  const tauri = window.__TAURI__;
  if (!tauri) return Promise.resolve(null);
  return tauri.core.invoke("app_info").catch(() => null);
};

// Makes `field` (an <input readonly> or <textarea readonly>) select itself when clicked, and
// `button` copy its text.
window.pastegetCopyable = (field, button) => {
  const select = () => {
    field.focus();
    field.select();
  };
  field.addEventListener("click", select);
  field.addEventListener("focus", select);
  if (!button) return;
  button.addEventListener("click", async () => {
    select();
    let copied = false;
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(field.value);
        copied = true;
      }
    } catch (_) {
      // Not allowed here: the selection is copied the old way below.
    }
    if (!copied) copied = document.execCommand("copy");
    if (copied) {
      const label = button.textContent;
      button.textContent = window.pastegetText("copied", "Copied");
      setTimeout(() => (button.textContent = label), 1500);
    }
  });
};
