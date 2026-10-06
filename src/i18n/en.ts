// English is the source catalog: every other locale must provide exactly these keys.
export const en = {
  "header.refresh": "Refresh",
  "header.refreshing": "Refreshing…",
  "header.settings": "Settings",

  "connect.title": "Connect Canvas",
  "connect.body":
    "Connect your school's Canvas account to see every Canvas and Gradescope deadline in one place.",
  "connect.searchLabel": "Find your school",
  "connect.searchPlaceholder": "Type your school's name",
  "connect.searching": "Searching…",
  "connect.noResults": "No schools found.",
  "connect.manualLink": "Can't find it? Enter your Canvas address",
  "connect.manualLabel": "Canvas address",
  "connect.manualPlaceholder": "canvas.yourschool.edu",
  "connect.manualSubmit": "Continue",
  "connect.checking": "Checking…",
  "connect.backToSearch": "Search by school name instead",
  "connect.privacy":
    "You sign in on your school's own page. Canvasist never sees your password, and everything stays on this computer.",

  "login.title": "Sign in to Canvas",
  "login.body":
    "Finish signing in in the window that just opened. It closes by itself when you're done.",
  "login.cancel": "Cancel",

  "list.label": "Assignments",
  "list.loading": "Loading assignments…",
  "list.empty": "No assignments this term.",
  "list.updated": "Updated {time}",
  "list.openInCanvas": "Open in Canvas",

  "status.notSubmitted": "Not submitted",
  "status.submitted": "Submitted",
  "status.late": "Late",
  "status.graded": "Graded",
  "status.missing": "Missing",
  "status.excused": "Excused",

  "banner.expired": "Your Canvas session has expired.",
  "banner.relogin": "Sign in again",

  "settings.title": "Settings",
  "settings.back": "Back",
  "settings.language": "Language",
  "settings.language.system": "System default",
  "settings.autostart": "Start when I sign in to Windows",
  "settings.autostart.hint": "Canvasist runs quietly in the tray so reminders keep working.",
  "settings.account": "Canvas account",
  "settings.account.connected": "Connected to {site}",
  "settings.signOut": "Sign out",
  "settings.signOut.hint":
    "Removes your saved session, cached assignments and login data from this computer.",
  "settings.signOut.confirm": "Sign out and delete all local data?",
  "settings.signOut.yes": "Sign out",
  "settings.signOut.no": "Cancel",
  "settings.about": "About",
  "settings.version": "Version {version}",

  "error.generic": "Something went wrong: {message}",
  "error.network": "Can't reach the server. Check your internet connection.",
  "error.invalidUrl": "That doesn't look like a valid web address.",
  "error.notCanvas": "No Canvas site was found at that address.",
  "error.sessionExpired": "Your Canvas session has expired. Please sign in again.",
  "error.canvas": "Canvas returned an error ({message}).",
  "error.busy": "A refresh is already in progress.",
} as const;

export type MessageKey = keyof typeof en;
