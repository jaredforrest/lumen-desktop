{ pkgs }:
let
  shell = "${pkgs.bash}/bin/bash";
  base = pkgs.speechd.override {
    withLibao = false;
    withFlite = false;
    withEspeak = false;
    withPico = false;
  };
  patched = base.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [
      (pkgs.replaceVars ./speech-dispatcher-generic-shell.patch {
        inherit shell;
      })
    ];
  });
  proof = pkgs.writeTextDir "share/lumen-fixtures/speech-shell.json" (
    builtins.toJSON {
      inherit shell;
      scope = "Test-only generic module execution shell; quoted payload and pipeline failure semantics unchanged";
    }
  );
in pkgs.symlinkJoin {
  name = "lumen-test-speech-dispatcher";
  paths = [ patched proof ];
}
