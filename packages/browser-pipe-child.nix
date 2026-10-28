{ pkgs }:
builtins.derivation {
  name = "lumen-browser-pipe-child";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./browser-pipe-child.nu}" "${../tests/desktop/browser_fixture/pipe_child.rs}" "${../tests/desktop/browser_fixture/invocation.rs}" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.rustc pkgs.stdenv.cc pkgs.coreutils ];
}
