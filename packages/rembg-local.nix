{ pkgs }:
# The desktop only needs single-file processing, never the optional web server.
(pkgs.python3Packages.rembg.override { withCli = false; }).overridePythonAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./rembg-local-command.patch ];
  dependencies = (old.dependencies or [ ]) ++ [ pkgs.python3Packages.click ];
  postInstall = "";
  postInstallCheck = (old.postInstallCheck or "") + ''
    "$out/bin/rembg" --version
    "$out/bin/rembg" i --help > "$TMPDIR/rembg-image-help"
    grep -q -- '--model' "$TMPDIR/rembg-image-help"
  '';
})
