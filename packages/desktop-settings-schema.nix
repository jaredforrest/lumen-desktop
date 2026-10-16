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
            dockMagnification = mkOption { type = types.bool; default = true; };
            scrollbars = mkOption { type = types.enum [ "auto" "always" ]; default = "auto"; };
            reduceTransparency = mkOption { type = types.bool; default = false; };
            reduceMotion = mkOption { type = types.bool; default = false; };
          };
        };
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
          policies = mkOption { type = types.attrsOf (types.enum [ "banners" "silent" "off" ]); default = {}; };
        }; };
        default = {};
      };
    };
  };
in (lib.evalModules { modules = [ schema { config = values; } ]; }).config
