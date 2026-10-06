// Prototype-pollution hardening for Canvasist's own UI.
//
// This is done here rather than with Tauri's global `freezePrototype` option,
// because Tauri injects that option into every webview, including the login
// window that shows the school's SSO and Duo pages, whose scripts break on a
// frozen Object.prototype. This module must be the first import in main.tsx.
Object.freeze(Object.prototype);
