{ pkgs, rofiPackage ? pkgs.rofi, characterViewer ? import ../res/character-viewer { inherit pkgs; } }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-utilities";
    version = "0.1.0";
    src = ../res/utilities/conveniences;
    cargoLock.lockFile = ../res/utilities/conveniences/Cargo.lock;
  };
in pkgs.writeTextFile {
  name = "lumen-utilities";
  destination = "/bin/lumen-utilities";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    const wrapper_path = path self
    def --wrapped main [...args] {
      $env.LUMEN_UTILITIES_WRAPPER = $wrapper_path
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.sway rofiPackage characterViewer pkgs.wl-clipboard pkgs.grim pkgs.slurp pkgs.satty pkgs.wf-recorder pkgs.systemd pkgs.libnotify pkgs.coreutils ])} | split row ':') ++ $env.PATH)
      exec ${binary}/bin/lumen-utilities ...$args
    }
  '';
}
