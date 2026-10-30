{ lib }:
lib.types.submodule {
  options.rebindings = lib.mkOption {
    default = {};
    type = lib.types.attrsOf (lib.types.nullOr (lib.types.addCheck lib.types.str
      (value: builtins.stringLength value > 0 && builtins.stringLength value <= 512 && builtins.match "[^[:cntrl:]]+" value != null)));
    description = "Stable owned binding identities mapped to explicit symbol chords; null disables a binding. Omission restores its configured default.";
  };
}
