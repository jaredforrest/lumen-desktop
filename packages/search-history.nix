{ pkgs, nixSettings }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-search-history";
    version = "0.1.0";
    src = ../res/lumen-desktop/search-history;
    cargoLock.lockFile = ../res/lumen-desktop/search-history/Cargo.lock;
  };
in pkgs.writeTextFile {
  name = "lumen-search-history";
  destination = "/bin/lumen-search-history";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.LUMEN_NIX_SETTINGS_BIN = "${nixSettings}/bin/lumen-nix-settings"
      exec ${binary}/bin/lumen-search-history ...$args
    }
  '';
}
