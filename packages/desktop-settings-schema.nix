{ lib, values, defaults ? import ../config/application-profiles.nix }:
let
  inherit (lib) mkOption types;
  valueType = value:
    if builtins.isBool value then types.bool
    else if builtins.isInt value then types.int
    else if builtins.isFloat value then types.number
    else if builtins.isString value then types.str
    else if value == null then types.nullOr types.anything
    else if builtins.isAttrs value then types.submodule {
      options = lib.mapAttrs (_: item: mkOption { type = valueType item; default = item; }) value;
    }
    else types.anything;
  normalizedValues = if values ? menuBar.fullscreenVisibility then
    if values.menuBar ? visibility then throw "Use only menuBar.visibility; remove the legacy fullscreenVisibility alias"
    else if !(builtins.elem values.menuBar.fullscreenVisibility [ "automatic" "always" ]) then throw "Invalid legacy menu visibility"
    else values // { menuBar = builtins.removeAttrs values.menuBar [ "fullscreenVisibility" ] // {
      visibility = if values.menuBar.fullscreenVisibility == "always" then "always" else "desktop";
    }; } else values;
  displayText = types.addCheck (types.strMatching "[^[:cntrl:]]+") (value: lib.stringLength value <= 512);
  validPrimaryDisplay = value: builtins.isAttrs value && (if (value.kind or "") == "connector" then
    (value.name or null) != null && (value.make or null) == null && (value.model or null) == null && (value.serial or null) == null
    else (value.kind or "") == "display" && (value.name or null) == null && (value.make or null) != null && (value.model or null) != null && (value.serial or null) != null
      && !(builtins.elem (value.serial or null) [ "Unknown" "0" ]));
  primaryDisplayType = types.addCheck (types.submodule { options = {
    kind = mkOption { type = types.enum [ "connector" "display" ]; };
    name = mkOption { type = types.nullOr displayText; default = null; };
    make = mkOption { type = types.nullOr displayText; default = null; };
    model = mkOption { type = types.nullOr displayText; default = null; };
    serial = mkOption { type = types.nullOr displayText; default = null; };
  }; }) validPrimaryDisplay;
  schema = {
    options = {
      applicationPreferences = mkOption { type = valueType defaults; default = defaults; };
      appearance = mkOption {
        type = types.submodule {
          options = {
            mode = mkOption { type = types.enum [ "auto" "light" "dark" ]; default = "auto"; };
            accent = mkOption { type = types.enum [ "blue" "purple" "pink" "red" "orange" "yellow" "green" "graphite" ]; default = "blue"; };
            iconAppearance = mkOption { type = types.enum [ "default" "auto" "dark" ]; default = "auto"; };
            switchToAppSpace = mkOption { type = types.bool; default = true; };
            dockPosition = mkOption { type = types.enum [ "bottom" "left" "right" ]; default = "bottom"; };
            dockMinimizeDestination = mkOption { type = types.enum [ "application" "windows" ]; default = "application"; };
            dockAutoHide = mkOption { type = types.bool; default = false; };
            dockSize = mkOption { type = types.ints.between 32 72; default = 48; };
            dockFiles = mkOption { type = types.addCheck (types.listOf (types.addCheck (types.strMatching "/[^[:cntrl:]]*") (path: lib.stringLength path <= 4096))) (paths: lib.length paths == lib.length (lib.unique paths)); default = []; };
            dockFolders = mkOption {
              type = types.addCheck (types.listOf (types.submodule { options = {
                path = mkOption { type = types.addCheck (types.strMatching "/[^[:cntrl:]]*") (path: lib.stringLength path <= 4096); };
                sort = mkOption { type = types.enum [ "name" "modified" "size" "type" ]; default = "modified"; };
                descending = mkOption { type = types.bool; default = true; };
                presentation = mkOption { type = types.enum [ "list" "grid" "auto" ]; default = "auto"; };
              }; })) (folders: lib.length folders == lib.length (lib.unique (map (folder: folder.path) folders)));
              default = [];
            };
            dockPins = mkOption {
              type = types.nullOr (types.addCheck (types.listOf (types.strMatching "[-_.A-Za-z0-9]+")) (pins: lib.length pins == lib.length (lib.unique pins) && lib.all (pin: lib.stringLength pin <= 256) pins));
              default = null;
            };
            dockRecentApps = mkOption { type = types.bool; default = true; };
            dockIndicators = mkOption { type = types.bool; default = true; };
            dockMagnification = mkOption { type = types.bool; default = true; };
            scrollbars = mkOption { type = types.enum [ "auto" "when_scrolling" "always" ]; default = "auto"; };
            reduceTransparency = mkOption { type = types.bool; default = false; };
            increaseContrast = mkOption { type = types.bool; default = false; };
            textScale = mkOption { type = types.enum [ 1 1.15 1.3 1.5 ]; default = 1; };
            sidebarIconSize = mkOption { type = types.enum [ "small" "medium" "large" ]; default = "medium"; };
            reduceMotion = mkOption { type = types.bool; default = false; };
          };
        };
        default = {};
      };
      spaces = mkOption {
        type = types.submodule { options = {
          separateDisplays = mkOption { type = types.bool; default = true; };
          primaryDisplay = mkOption { type = types.nullOr primaryDisplayType; default = null;
            description = "Main display identity; null retains the automatically selected stable identity."; };
        }; };
        default = {};
      };
      desktop = mkOption {
        type = types.submodule { options = {
          showFiles = mkOption { type = types.bool; default = false; };
          fileSort = mkOption { type = types.enum [ "name" "modified" "size" "kind" ]; default = "name"; };
          fileDescending = mkOption { type = types.bool; default = false; };
        }; };
        default = {};
      };
      windows = mkOption {
        type = types.submodule { options = {
          rememberOwnedGeometry = mkOption { type = types.bool; default = false; };
          tileMargins = mkOption { type = types.bool; default = true; };
          tileGap = mkOption { type = types.ints.between 0 32; default = 8; };
          edgeTiling = mkOption { type = types.bool; default = true; };
          optionTiling = mkOption { type = types.bool; default = true; };
          topEdgeFill = mkOption { type = types.bool; default = true; };
          topEdgeOverview = mkOption { type = types.bool; default = true; };

          titlebarDoubleClick = mkOption { type = types.enum [ "fill" "zoom" "minimize" "none" ]; default = "fill"; };
        }; };
        default = {};
      };
      menuBar = mkOption {
        type = types.submodule { options = let
          ids = [ "battery" "sound" "focus" "search" "controls" ];
        in {
          order = mkOption { type = types.addCheck (types.listOf (types.enum ids)) (items: lib.length items == lib.length ids && lib.length items == lib.length (lib.unique items)); default = ids; };
          visibility = mkOption { type = types.enum [ "always" "desktop" "fullscreen" "auto_hide" ]; default = "desktop"; description = "Contexts where the menu remains visible; auto_hide reveals at the top edge everywhere."; };
          hourCycle = mkOption { type = types.enum [ "locale" "12" "24" ]; default = "locale"; };
          locale = mkOption { type = types.strMatching "^$|[a-z]{2,3}(_[A-Z]{2})?"; default = ""; };
          showDate = mkOption { type = types.bool; default = true; };
          showWeekday = mkOption { type = types.bool; default = true; };
          showSeconds = mkOption { type = types.bool; default = false; };
        }; };
        default = {};
      };
      controlCentre = mkOption {
        type = types.submodule {
          options = let
            ids = [ "wifi" "bluetooth" "sharing" "media" "displays" "appearance" "focus" "brightness" "sound" ];
            idList = types.listOf (types.enum ids);
            unique = items: lib.length items == lib.length (lib.unique items);
          in {
            order = mkOption { type = types.addCheck idList (items: unique items && lib.length items == lib.length ids); default = ids; };
            hidden = mkOption { type = types.addCheck idList unique; default = []; };
          };
        };
        default = {};
      };
      search = mkOption {
        type = types.submodule { options = {
          queryHistory = mkOption { type = types.bool; default = false; };
          disabledProviders = mkOption {
            type = types.addCheck (types.listOf (types.enum [ "applications" "files" "actions" "clipboard" ])) (items: lib.length items == lib.length (lib.unique items));
            default = [];
          };
        }; };
        default = {};
      };
      screenSaver = mkOption {
        type = types.submodule { options = {
          enabled = mkOption { type = types.bool; default = false; };
          idleSeconds = mkOption { type = types.ints.between 15 7200; default = 300; };
          motion = mkOption { type = types.bool; default = true; };
        }; };
        default = {};
      };
      wallpaper = mkOption {
        type = types.submodule { options = let
          imagePath = types.addCheck types.str (value: lib.hasPrefix "/" value && !lib.hasInfix "\n" value && !lib.hasInfix "\r" value);
        in {
          defaultImage = mkOption { type = types.nullOr imagePath; default = null; };
          displays = mkOption { type = types.attrsOf (types.submodule { options = {
            identity = mkOption { type = types.str; };
            image = mkOption { type = imagePath; };
          }; }); default = {}; };
        }; };
        default = {};
      };
      inputs = mkOption {
        type = types.addCheck (types.attrsOf (types.submodule {
          options = let
            optional = type: mkOption { type = types.nullOr type; default = null; };
            fields = {
              repeat_rate = optional (types.ints.between 0 60);
              repeat_delay = optional (types.ints.between 100 2000);
              pointer_accel = optional (types.addCheck types.number (v: v >= -1 && v <= 1));
              scroll_factor = optional (types.addCheck types.number (v: v >= 0.1 && v <= 5));
              natural_scroll = optional types.bool;
              left_handed = optional types.bool;
              tap = optional types.bool;
              dwt = optional types.bool;
            };
          in {
            identity = mkOption { type = types.submodule { options = {
              identifier = mkOption { type = types.strMatching "[0-9]+:[0-9]+:[A-Za-z0-9_.:+()-]+"; };
              name = mkOption { type = types.str; };
              type = mkOption { type = types.enum [ "keyboard" "pointer" "touchpad" ]; };
            }; }; };
            values = mkOption { type = types.submodule { options = fields; }; default = {}; };
            baseline = mkOption { type = types.submodule { options = fields; }; default = {}; };
          };
        })) (profiles: lib.all (key: key == builtins.hashString "sha256" (builtins.toJSON profiles.${key}.identity)) (builtins.attrNames profiles));
        default = {};
      };
      displays = mkOption {
        type = types.attrsOf (types.submodule {
          options = {
            name = mkOption { type = types.str; };
            identity = mkOption { type = types.listOf (types.nullOr types.str); };
            mode = mkOption { type = types.strMatching "[0-9]+x[0-9]+(@[0-9.]+Hz)?"; };
            scale = mkOption { type = types.enum [ 1 1.25 1.5 1.75 2 2.5 3 ]; };
            x = mkOption { type = types.ints.between (-32768) 32767; };
            y = mkOption { type = types.ints.between (-32768) 32767; };
          };
        });
        default = {};
      };
      textTools = mkOption {
        type = types.submodule {
          options = {
            automaticReplacements = mkOption { type = types.bool; default = true; };
            language = mkOption { type = types.enum [ "en_GB" "en_US" ]; default = "en_GB"; };
          };
        };
        default = {};
      };
      menuShortcuts = mkOption {
        type = types.attrsOf (types.submodule {
          options = {
            app = mkOption { type = types.str; };
            path = mkOption { type = types.str; };
          };
        });
        default = {};
      };
      shortcuts = mkOption { type = import ./shortcut-preferences-type.nix { inherit lib; }; default = {}; };
      notifications = mkOption {
        type = types.submodule { options = {
          preview = mkOption { type = types.enum [ "always" "when_unlocked" "never" ]; default = "when_unlocked"; };
          policies = mkOption { type = types.attrsOf (types.enum [ "banners" "silent" "off" ]); default = {}; };
        }; };
        default = {};
      };
    };
  };
in let
  result = (lib.evalModules { modules = [ schema { config = normalizedValues; } ]; }).config;
  folders = result.appearance.dockFolders;
  # Submodule list merges validate their members individually. Enforce the
  # collection invariant on the fully evaluated result as well.
  distinctFolders = lib.length folders == lib.length (lib.unique (map (folder: folder.path) folders));
  inputFields = kind: if kind == "keyboard" then [ "repeat_rate" "repeat_delay" ]
    else [ "pointer_accel" "scroll_factor" "natural_scroll" "left_handed" ] ++ lib.optionals (kind == "touchpad") [ "tap" "dwt" ];
  validInputs = lib.all (key: let
    profile = result.inputs.${key};
    active = builtins.filter (field: profile.values.${field} != null) (builtins.attrNames profile.values);
  in key == builtins.hashString "sha256" (builtins.toJSON profile.identity)
    && lib.all (field: builtins.elem field (inputFields profile.identity.type) && profile.baseline.${field} != null) active
  ) (builtins.attrNames result.inputs);
  validWallpapers = lib.all (key: key == builtins.hashString "sha256" result.wallpaper.displays.${key}.identity)
    (builtins.attrNames result.wallpaper.displays);
in if result.spaces.primaryDisplay != null && !validPrimaryDisplay result.spaces.primaryDisplay then throw "Primary display fields must match their identity kind"
  else if !validWallpapers then throw "Wallpaper display keys must match their identities"
  else if !distinctFolders then throw "Dock folder paths must be distinct"
  else if !validInputs then throw "Input profile keys, device option scope and previous values must match their identities"
  else result
