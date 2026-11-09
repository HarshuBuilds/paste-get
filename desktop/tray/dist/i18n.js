// SPDX-License-Identifier: GPL-3.0-or-later
// French for the tray's windows when the system is in French. Elements with data-i18n="key"
// get the French text of that key; English stays in the HTML.
"use strict";

window.pastegetFrench = (navigator.language || "").toLowerCase().startsWith("fr");

window.pastegetFr = {
  // pair.html
  pairTitle: "Appairer un téléphone",
  pairStep1: "Sur ton téléphone, ouvre paste-get et appuie sur Appairer un ordinateur.",
  pairStep2: "Scanne ce code.",
  pairPreparing: "Préparation du code…",
  pairOnce: "Le code ne sert qu'une fois, pendant quelques minutes.",
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
});
