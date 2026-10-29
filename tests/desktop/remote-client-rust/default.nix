{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-remote-client-fixture";
  version = "0.1.0";
  src = ../.;
  cargoRoot = "remote-client-rust";
  buildAndTestSubdir = "remote-client-rust";
  cargoLock.lockFile = ./Cargo.lock;
  buildInputs = [ pkgs.gtk3 pkgs.cairo pkgs.glib ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postFixup = ''
    wrapProgram $out/bin/lumen-remote-client-fixture --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git pkgs.coreutils pkgs.dbus pkgs.bash pkgs.grim ]}
  '';
}
