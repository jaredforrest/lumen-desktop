{ pkgs, lib ? pkgs.lib, swayPackage ? pkgs.sway, user ? "user", displayName ? user, assets ? ../res/lumen-desktop, greeterExtraConfig ? "", allowUnsupportedGpu ? false }:
let
  quickshell = import ./quickshell-runtime.nix { inherit pkgs; };
  lockLauncher = import ./lock-launcher.nix { inherit pkgs; };
  authAssets = builtins.derivation {
    name = "lumen-auth-assets";
    system = pkgs.stdenv.hostPlatform.system;
    builder = "${pkgs.nushell}/bin/nu";
    args = [ "${./auth-assets.nu}" "${assets}" ];
    PATH = pkgs.lib.makeBinPath [ pkgs.coreutils ];
  };
  fontConfig = (import ./font-configuration.nix { inherit pkgs; }) {
    fontDirectories = [
      "${pkgs.callPackage ./interlude.nix {}}/share/fonts"
      "${pkgs.dejavu_fonts}/share/fonts"
    ];
  };
  lock = pkgs.writeTextFile {
    name = "lumen-lock";
    destination = "/bin/lumen-lock";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def main [] {
        $env.FONTCONFIG_FILE = "${fontConfig}"
        $env.LUMEN_AUTH_NAME = ${builtins.toJSON displayName}
        exec ${lockLauncher}/bin/lumen-lock-launcher ${quickshell}/bin/quickshell ${pkgs.swaylock}/bin/swaylock ${authAssets}/lock.qml
      }
    '';
  };
  greeterClient = pkgs.writeTextFile {
    name = "lumen-greeter-client";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def main [] {
        $env.FONTCONFIG_FILE = "${fontConfig}"
        try {
          ^${quickshell}/bin/quickshell --path ${authAssets}/greeter.qml
          ^${pkgs.coreutils}/bin/touch $env.LUMEN_AUTH_SUCCESS
        } catch {}
        ^${swayPackage}/bin/swaymsg exit
      }
    '';
  };
  swayConfig = pkgs.writeText "lumen-greeter-sway.conf" ''
    xwayland disable
    output * bg #080b0f solid_color
    ${greeterExtraConfig}
    seat * hide_cursor 3000
    exec ${greeterClient}
  '';
  greeter = pkgs.writeTextFile {
    name = "lumen-greeter";
    destination = "/bin/lumen-greeter";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def main [] {
        $env.LUMEN_AUTH_NAME = ${builtins.toJSON displayName}
        $env.LUMEN_AUTH_USER = ${builtins.toJSON user}
        $env.LUMEN_AUTH_SESSION = "${swayPackage}/bin/sway"
        $env.LUMEN_AUTH_UNSUPPORTED_GPU = "${if allowUnsupportedGpu then "1" else "0"}"
        ${lib.optionalString allowUnsupportedGpu ''$env.SWAY_UNSUPPORTED_GPU = "1"''}
        $env.XDG_CURRENT_DESKTOP = "sway"
        let marker = (^${pkgs.coreutils}/bin/mktemp | str trim)
        $env.LUMEN_AUTH_SUCCESS = $marker
        ^${pkgs.coreutils}/bin/rm $marker
        try { ^${swayPackage}/bin/sway ${lib.optionalString allowUnsupportedGpu "--unsupported-gpu"} --config ${swayConfig} } catch {}
        if ($marker | path exists) {
          ^${pkgs.coreutils}/bin/rm -f $marker
          exit 0
        }
        ^${pkgs.coreutils}/bin/rm -f $marker
        exec ${pkgs.tuigreet}/bin/tuigreet --time --remember --asterisks --cmd ${swayPackage}/bin/sway
      }
    '';
  };
  greeterConfig = (pkgs.formats.toml {}).generate "greetd.toml" {
    default_session = { command = "${greeter}/bin/lumen-greeter"; user = "greeter"; };
    terminal.vt = 1;
  };
  activateLogin = pkgs.writeTextFile {
    name = "lumen-auth-activate-login";
    destination = "/bin/lumen-auth-activate-login";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def --wrapped main [...args] {
        if (^${pkgs.coreutils}/bin/id -u | str trim) != "0" {
          print --stderr "Run this command with administrator privileges."
          exit 1
        }
        let directory = "/run/systemd/system/greetd.service.d"
        ^${pkgs.coreutils}/bin/install -d -m 0755 $directory
        $"[Service]\nExecStart=\nExecStart=${pkgs.greetd}/bin/greetd --config ${greeterConfig}\n" | save --force ($directory | path join 90-lumen-auth.conf)
        ^${pkgs.systemd}/bin/systemctl daemon-reload
        print "Login screen configured for the next greetd restart."
        if ($args.0? | default "") == "--restart" {
          ^${pkgs.systemd}/bin/systemctl restart greetd
        }
      }
    '';
  };
in { inherit lock greeter greeterClient fontConfig activateLogin; assets = authAssets; }
