// English is the source catalog: every other locale must provide exactly these keys.
export const en = {
  "header.refresh": "Refresh",
  "header.refreshUnavailable": "Connect Canvas to refresh",
  "header.settings": "Settings",

  "empty.title": "Canvas is not connected",
  "empty.body":
    "Connect your school's Canvas account to see every Canvas and Gradescope deadline in one place.",
  "empty.connect": "Connect Canvas",
  "empty.comingSoon": "Coming in the next version",

  "settings.title": "Settings",
  "settings.back": "Back",
  "settings.language": "Language",
  "settings.language.system": "System default",
  "settings.autostart": "Start when I sign in to Windows",
  "settings.autostart.hint": "Canvasist runs quietly in the tray so reminders keep working.",
  "settings.about": "About",
  "settings.version": "Version {version}",

  "error.generic": "Something went wrong: {message}",
} as const;

export type MessageKey = keyof typeof en;
