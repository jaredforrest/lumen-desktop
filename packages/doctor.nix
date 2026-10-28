{ pkgs, emojiFamily }:
let
  runtime = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-desktop-doctor-runtime";
    version = "0.1.0";
    src = ../res/lumen-desktop/doctor;
    cargoLock.lockFile = ../res/lumen-desktop/doctor/Cargo.lock;
  };
in pkgs.writeTextFile {
  name = "lumen-desktop-doctor";
  destination = "/bin/lumen-desktop-doctor";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.systemd pkgs.fontconfig pkgs.glib ])} | split row ':') ++ $env.PATH)
      $env.LUMEN_EMOJI_FONT_FAMILY = ${builtins.toJSON emojiFamily}
      $env.GSETTINGS_SCHEMA_DIR = "${pkgs.glib.getSchemaPath pkgs.gsettings-desktop-schemas}"
      exec ${runtime}/bin/lumen-desktop-doctor ...$args
    }
  '';
}
