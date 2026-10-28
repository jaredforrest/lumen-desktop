{ pkgs }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-file-preview";
    version = "0.1.0";
    src = ../res/lumen-desktop/file-preview;
    cargoLock.lockFile = ../res/lumen-desktop/file-preview/Cargo.lock;
    nativeCheckInputs = [ pkgs.coreutils pkgs.util-linux ];
  };
in pkgs.writeTextFile {
  name = "lumen-file-preview";
  destination = "/bin/lumen-file-preview";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.util-linux pkgs.poppler-utils pkgs.ffmpeg-headless ])} | split row ':') ++ $env.PATH)
      exec ${binary}/bin/lumen-file-preview ...$args
    }
  '';
}
