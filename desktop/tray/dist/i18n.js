// SPDX-License-Identifier: GPL-3.0-or-later
// French for the tray's windows when the system is in French. Elements with data-i18n="key"
// get the French text of that key (data-i18n-placeholder and data-i18n-label: their placeholder
// and aria-label); English stays in the HTML and the scripts. desktop/tray/src/i18n_check.rs
// checks that every key used has its French, and every French text is used.
"use strict";

// The tray says which language it speaks (main.rs, language_script); a page opened elsewhere
// follows the browser.
window.pastegetFrench = window.pastegetLang
  ? window.pastegetLang === "fr"
  : (navigator.language || "").toLowerCase().startsWith("fr");

window.pastegetFr = {
  // pair.html, pair.js
  pairTitle: "Appairer un téléphone",
  pairStep1: "Sur ton téléphone, ouvre paste-get et touche Scanner le code (ou Appareils → Appairer un ordinateur).",
  pairStep2: "Scanne ce code.",
  pairCodeLabel: "Code QR d'appairage",
  pairPreparing: "Préparation du code…",
  pairOnce: "Le code ne sert qu'une fois. Garde-le pour toi : le premier qui le scanne est appairé.",
  pairRenews: "Un nouveau code arrive dans {time}.",
  pairPaired: "Appairé avec {name}.",
  pairRetrying: "Nouvel essai dans quelques secondes…",
  pairGetApp: "Pas encore paste-get sur ton téléphone ? Télécharge le fichier qui finit par _foss.apk depuis cette page, sur le téléphone :",
  pairDownloadLabel: "Page de téléchargement",
  // settings.html
  settingsTitle: "Réglages",
  nameLabel: "Nom de cet ordinateur",
  nameHint: "Affiché sur ton téléphone et sur tes autres ordinateurs.",
  notificationsTitle: "Notifications",
  notifyLegend: "En afficher une quand quelque chose arrive :",
  notifyImages: "Images seulement",
  notifyAll: "Images et texte",
  notifyOff: "Jamais",
  links: "Proposer d'ouvrir les liens copiés sur un autre appareil",
  codesHint: "Les codes 2FA signalent toujours leur arrivée, sauf si les notifications sont coupées. Jamais avec le code lui-même.",
  hotkeysTitle: "Raccourcis",
  pauseLabel: "Mettre en pause ou reprendre",
  sendLabel: "Envoyer le presse-papiers",
  hotkeysHint: "Clique dans un champ et appuie sur les touches, par exemple Ctrl+Alt+Shift+P. Retour arrière le désactive.",
  hotkeyNone: "Aucun",
  networkTitle: "Réseau",
  networkCommandLabel: "Lignes à lancer en administrateur",
  hyprlandTitle: "Hyprland",
  hyprlandHint: "Hyprland lance paste-get à l'ouverture de session et gère ses raccourcis dans sa propre configuration. Ajoute ces lignes à ~/.config/hypr/hyprland.conf, puis lance hyprctl reload. Copie-les à nouveau après avoir changé un raccourci.",
  hyprlandLabel: "Lignes pour hyprland.conf",
  login: "Lancer paste-get à l'ouverture de session",
  close: "Fermer",
  save: "Enregistrer",
  saving: "Enregistrement…",
  badKey: "Un raccourci utilise une lettre, un chiffre ou F1 à F24, avec Ctrl, Alt ou Super.",
  outsideApp: "cette page ne marche que dans paste-get",
  // about.html
  aboutTagline: "Copie sur ton téléphone, colle sur ton PC.",
  aboutLicence: "Logiciel libre sous licence publique générale GNU, version 3 ou ultérieure (GPL-3.0-or-later). Il est fourni sans aucune garantie.",
  aboutSource: "Code source",
  aboutOffline: "paste-get ne va jamais sur internet de lui-même : colle l'adresse dans ton navigateur.",
  // copy.js
  copy: "Copier",
  copied: "Copié",
};

// French text for `key`, or `english`.
window.pastegetText = (key, english) =>
  (window.pastegetFrench && window.pastegetFr[key]) || english;

// The same, with each {name} replaced by values.name. Values are text: never parsed again.
window.pastegetFormat = (key, english, values) =>
  window
    .pastegetText(key, english)
    .replace(/\{(\w+)\}/g, (match, name) => (name in values ? String(values[name]) : match));

document.addEventListener("DOMContentLoaded", () => {
  if (!window.pastegetFrench) return;
  document.documentElement.lang = "fr";
  document.querySelectorAll("[data-i18n]").forEach((element) => {
    const text = window.pastegetFr[element.dataset.i18n];
    if (text) element.textContent = text;
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach((element) => {
    const text = window.pastegetFr[element.dataset.i18nPlaceholder];
    if (text) element.placeholder = text;
  });
  document.querySelectorAll("[data-i18n-label]").forEach((element) => {
    const text = window.pastegetFr[element.dataset.i18nLabel];
    if (text) element.setAttribute("aria-label", text);
  });
});
