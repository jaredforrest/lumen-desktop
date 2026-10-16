# Remembered working state and its first-launch defaults; never config policy.
{
  prism = import ./prism-state-fields.nix;
  wordcraft."ui.json" = {
    readAloudRate = 1.0;
    editing.markup = { comments = true; formatting = true; ink = true; insertionsDeletions = true; };
    backstage = false; backstagePage = "home"; navTab = "headings";
    recent = []; recentSymbols = []; ribbonCollapsed = false; tab = "Home"; window = null;
  };
  gridcraft = {
    "ui.json" = { formulaBar = true; statusBar = true; formulaBarExpanded = false; recent = []; ribbonCollapsed = false; ribbonTab = "Home"; };
    "prefs.json" = { userDictionary = []; };
  };
  deckcraft = {
    "ui.json" = {
      formatTab = "fill"; gridlines = false; guides = false; notes = true; notesHeight = 52.0;
      pane = null; paneWidth = 300.0; recent = []; ribbonCollapsed = false; ruler = false;
      sorterZoom = 1.0; tab = "Home"; thumbsWidth = 236.0; view = "normal"; zoom = null;
    };
    "prefs.json" = {};
  };
}
