.pragma library
// Functional launch slots; unassigned slots are omitted by the model.
var items = [
  {
    "key": "files",
    "name": "Files",
    "appId": null,
    "center": 48,
    "action": "files",
    "image": "files.svg",
    "canvas": 67,
    "bottom": 14
  },
  {
    "key": "applications",
    "name": "Applications",
    "appId": null,
    "center": 121,
    "action": "launcher",
    "image": "applications.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "browser",
    "name": "Browser",
    "appId": "brave-browser",
    "center": 194,
    "action": null,
    "image": "browser.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "messages",
    "name": "Messages",
    "appId": null,
    "center": 267,
    "action": null,
    "image": "messages.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "mail",
    "name": "Mail",
    "appId": null,
    "center": 341,
    "action": null,
    "image": "mail.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "maps",
    "name": "Maps",
    "appId": null,
    "center": 414,
    "action": null,
    "image": "maps.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "photos",
    "name": "Photos",
    "appId": "org.darktable.darktable",
    "center": 487,
    "action": null,
    "image": "photos.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "calls",
    "name": "Calls",
    "appId": null,
    "center": 561,
    "action": null,
    "image": "calls.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "phone",
    "name": "Phone",
    "appId": null,
    "center": 634,
    "action": null,
    "image": "phone.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "calendar",
    "name": "Calendar",
    "appId": null,
    "center": 707,
    "action": "calendar",
    "image": "calendar.svg",
    "canvas": 62,
    "bottom": 16
  },
  {
    "key": "contacts",
    "name": "Contacts",
    "appId": "org.gnome.Contacts",
    "center": 781,
    "action": null,
    "image": "contacts.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "reminders",
    "name": "Reminders",
    "appId": null,
    "center": 854,
    "action": null,
    "image": "reminders.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "notes",
    "name": "Notes",
    "appId": "obsidian",
    "center": 927,
    "action": null,
    "image": "notes.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "freeform",
    "name": "Freeform",
    "appId": "com.github.xournalpp.xournalpp",
    "center": 1001,
    "action": null,
    "image": "freeform.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "tv",
    "name": "TV",
    "appId": "mpv",
    "center": 1074,
    "action": null,
    "image": "tv.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "music",
    "name": "Music",
    "appId": "audacity",
    "center": 1147,
    "action": null,
    "image": "music.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "keynote",
    "name": "Keynote",
    "appId": "impress",
    "center": 1221,
    "action": null,
    "image": "keynote.svg",
    "canvas": 70,
    "bottom": 12
  },
  {
    "key": "numbers",
    "name": "Numbers",
    "appId": "calc",
    "center": 1294,
    "action": null,
    "image": "numbers.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "pages",
    "name": "Pages",
    "appId": "writer",
    "center": 1367,
    "action": null,
    "image": "pages.svg",
    "canvas": 68,
    "bottom": 12
  },
  {
    "key": "games",
    "name": "Games",
    "appId": null,
    "center": 1441,
    "action": null,
    "image": "games.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "appstore",
    "name": "App Store",
    "appId": null,
    "center": 1514,
    "action": null,
    "image": "appstore.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "iphone",
    "name": "iPhone Mirroring",
    "appId": "iphone-mirror",
    "center": 1588,
    "action": null,
    "image": "iphone.svg",
    "canvas": 62,
    "bottom": 16
  },
  {
    "key": "settings",
    "name": "System Settings",
    "appId": null,
    "center": 1661,
    "action": "settings",
    "image": "settings.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "news",
    "name": "News",
    "appId": null,
    "center": 1734,
    "action": null,
    "image": "news.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "feedback",
    "name": "Feedback Assistant",
    "appId": null,
    "center": 1807,
    "action": null,
    "image": "feedback.svg",
    "canvas": 54,
    "bottom": 20
  },
  {
    "key": "utility-a",
    "name": "Preview",
    "appId": "org.gnome.Evince",
    "center": 1914,
    "action": null,
    "image": "utility-a.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "downloads",
    "name": "Downloads",
    "appId": null,
    "center": 1987,
    "action": "downloads",
    "image": "downloads.svg",
    "canvas": 68,
    "bottom": 13
  },
  {
    "key": "trash",
    "name": "Trash",
    "appId": null,
    "center": 2094,
    "action": "trash",
    "image": "trash.svg",
    "canvas": 66.28571428571429,
    "bottom": 13
  }
];
