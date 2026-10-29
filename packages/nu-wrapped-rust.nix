{ pkgs }:
{ name, binary, binaryName ? name, runtimeInputs ? [ ], environment ? { } }:
pkgs.writeTextFile {
  inherit name;
  destination = "/bin/${name}";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      ${pkgs.lib.optionalString (runtimeInputs != [ ]) ''$env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath runtimeInputs)} | split row ':') ++ $env.PATH)''}
      ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (key: value: ''$env.${key} = ${builtins.toJSON value}'') environment)}
      exec ${binary}/bin/${binaryName} ...$args
    }
  '';
}
