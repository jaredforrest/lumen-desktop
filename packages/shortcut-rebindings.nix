{ lib, base, defaults, rebindings }:
let
  eligible = row: builtins.hasAttr row.chord base && !(lib.hasPrefix "shell.app_switcher" row.action)
    && !(lib.hasPrefix "--" row.chord);
  known = builtins.filter eligible defaults;
  ids = map (row: row.id) known;
  unknown = builtins.filter (id: !(builtins.elem id ids)) (builtins.attrNames rebindings);
  rows = map (row: row // {
    defaultChord = row.chord;
    chord = rebindings.${row.id} or row.chord;
    enabled = (rebindings.${row.id} or row.chord) != null;
    command = base.${row.chord};
  }) known;
  retired = builtins.listToAttrs (map (row: {name = row.defaultChord; value = null;}) rows);
  requested = map (row: {
    inherit (row) id action enabled;
    chord = if row.chord == null then row.defaultChord else row.chord;
    scope = "global"; owner = "desktop";
  }) rows;
in if unknown != [] then throw "Unknown or protected shortcut rebinding identities: ${lib.concatStringsSep ", " unknown}"
else {
  inherit rows requested;
  bindings = base // retired // builtins.listToAttrs (map (row: {name = row.chord; value = row.command;}) (builtins.filter (row: row.enabled) rows));
}
