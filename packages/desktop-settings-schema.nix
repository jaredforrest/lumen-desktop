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
            sidebarIconSize = mkOption { type = types.enum [ "small" "medium" "large" ]; default = "medium"; };
            reduceMotion = mkOption { type = types.bool; default = false; };
          };
        };
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
          tileMargins = mkOption { type = types.bool; default = true; };
          tileGap = mkOption { type = types.ints.between 0 32; default = 8; };
          edgeTiling = mkOption { type = types.bool; default = true; };
          optionTiling = mkOption { type = types.bool; default = true; };
          topEdgeFill = mkOption { type = types.bool; default = true; };
          topEdgeOverview = mkOption { type = types.bool; default = true; };

          titlebarDoubleClick = mkOption { type = types.enum [ "fill" "minimize" "none" ]; default = "fill"; };
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
      notifications = mkOption {
        type = types.submodule { options = {
          preview = mkOption { type = types.enum [ "always" "never" ]; default = "always"; };
          policies = mkOption { type = types.attrsOf (types.enum [ "banners" "silent" "off" ]); default = {}; };
        }; };
        default = {};
      };
    };
  };
in let
  result = (lib.evalModules { modules = [ schema { config = values; } ]; }).config;
  folders = result.appearance.dockFolders;
  # Submodule list merges validate their members individually. Enforce the
  # collection invariant on the fully evaluated result as well.
  distinctFolders = lib.length folders == lib.length (lib.unique (map (folder: folder.path) folders));
in if !distinctFolders then throw "Dock folder paths must be distinct" else result
