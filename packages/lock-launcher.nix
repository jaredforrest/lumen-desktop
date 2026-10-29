{ pkgs }:
builtins.derivation {
  name = "lumen-lock-launcher";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./lock-launcher.nu}" "${../res/lumen-desktop/auth/lock-launcher.rs}" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.rustc pkgs.stdenv.cc pkgs.coreutils ];
}
