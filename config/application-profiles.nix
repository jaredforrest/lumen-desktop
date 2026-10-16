# Fields here declare behaviour. Working-state fields are listed separately.
{
  prism = import ./prism-preferences.nix;
  wordcraft = {
    "ui.json" = {
      author = "WordCraft User"; darkPage = false; language = "auto";
      readAloudSkipCitations = true; showDiscord = true; theme = "system";
      editing = {
        countNotes = true;
        markup = {
          balloons = "commentsAndFormatting"; changedLines = "outside";
          changedLinesColor = null; deleteColor = null;
          deleteMark = "strikethrough";
          insertColor = null; insertMark = "underline";
          trackFormatting = true;
        };
      };
    };
  };
  gridcraft = {
    "ui.json" = { dark = false; language = "en"; systemTheme = true; };
    "prefs.json" = {
      autocomplete = true; defaultFont = "Calibri"; defaultFontSize = 11.0;
      enterDirection = "down"; r1c1 = false; sheetsInNewWorkbook = 1; userName = "GridCraft User";
    };
  };
  deckcraft = {
    "ui.json" = { brightness = "light"; grayscale = false; presenterView = true; smartGuides = true; uiScale = 1.0; };
    "prefs.json" = {
      author = "Presenter"; autocorrect = true; duplicateOffset = 12.0;
      gridSpacing = 6.0; recoveryMinutes = 1.0; smartGuides = true;
      smartQuotes = true; snapToGrid = true; undoLimit = 500;
    };
  };
}
