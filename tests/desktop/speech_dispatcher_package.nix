{ pkgs }:
let
  base = pkgs.speechd.override { withLibao = false; withFlite = false; withEspeak = false; withPico = false; };
  shell = "${pkgs.bash}/bin/bash";
in base.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    ${pkgs.python3}/bin/python3 - <<'PY'
from pathlib import Path
p=Path('src/modules/generic.c')
s=p.read_text()
old='execl("/bin/sh", "sh", "-c", command, (char *) NULL)'
assert s.count(old)==1, 'Review changed generic execution boundary'
s=s.replace(old,'execl("${shell}", "bash", "-c", command, (char *) NULL)')
p.write_text(s)
PY
  '';
  postInstall = (old.postInstall or "") + ''
    mkdir -p "$out/share/lumen-fixtures"
    cat > "$out/share/lumen-fixtures/speech-shell.json" <<'JSON'
{"shell":"${shell}","scope":"Test-only generic module execution shell; quoted payload and pipeline failure semantics unchanged"}
JSON
  '';
})
