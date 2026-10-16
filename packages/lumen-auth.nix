{ pkgs, lib ? pkgs.lib, swayPackage ? pkgs.sway, user ? "user", displayName ? user }:
let
  assets = pkgs.runCommand "lumen-auth-assets" {} ''
    mkdir -p $out
    cp ${../res/lumen-desktop/auth}/*.qml $out/
    ln -s ${../res/lumen-desktop/shell/wallpaper.png} $out/wallpaper.png
  '';
  fontConfig = pkgs.makeFontsConf {
    fontDirectories = [
      "${pkgs.callPackage ./interlude.nix {}}/share/fonts"
      "${pkgs.dejavu_fonts}/share/fonts"
    ];
  };
  lock = pkgs.writeShellApplication {
    name = "lumen-lock";
    runtimeInputs = [ pkgs.quickshell pkgs.util-linux pkgs.coreutils pkgs.swaylock ];
    text = ''
      export FONTCONFIG_FILE=${fontConfig}
      export LUMEN_AUTH_NAME=${lib.escapeShellArg displayName}
      # Serialize startup, then return only once the compositor confirms coverage.
      exec 9>"$XDG_RUNTIME_DIR/lumen-lock-startup"
      flock 9
      if ! quickshell --no-duplicate --daemonize --path ${assets}/lock.qml 9>&-; then
        exec swaylock -f
      fi
      for _ in {1..100}; do
        if [[ $(quickshell --path ${assets}/lock.qml ipc call auth secure 2>/dev/null) == true ]]; then
          exit 0
        fi
        sleep 0.05
      done
      echo "The session locker did not confirm secure coverage" >&2
      exit 1
    '';
  };
  greeterClient = pkgs.writeShellScript "lumen-greeter-client" ''
    export FONTCONFIG_FILE=${fontConfig}
    if ${pkgs.quickshell}/bin/quickshell --path ${assets}/greeter.qml; then
      touch "$LUMEN_AUTH_SUCCESS"
    fi
    ${swayPackage}/bin/swaymsg exit
  '';
  swayConfig = pkgs.writeText "lumen-greeter-sway.conf" ''
    xwayland disable
    output * bg #080b0f solid_color
    input * xkb_layout gb
    input * xkb_options caps:swapescape
    input * xkb_numlock enabled
    seat * hide_cursor 3000
    exec ${greeterClient}
  '';
  greeter = pkgs.writeShellApplication {
    name = "lumen-greeter";
    runtimeInputs = [ pkgs.coreutils ];
    text = ''
      export LUMEN_AUTH_NAME=${lib.escapeShellArg displayName}
      export LUMEN_AUTH_USER=${lib.escapeShellArg user}
      export LUMEN_AUTH_SESSION=${swayPackage}/bin/sway
      export LUMEN_AUTH_HYPRLAND=${pkgs.hyprland}/bin/Hyprland
      export SWAY_UNSUPPORTED_GPU=1
      export XDG_CURRENT_DESKTOP=sway
      # Use the known working compositor, including its NVIDIA acknowledgement.
      # A compositor startup failure falls back to the existing password greeter.
      LUMEN_AUTH_SUCCESS=$(mktemp)
      export LUMEN_AUTH_SUCCESS
      rm "$LUMEN_AUTH_SUCCESS"
      trap 'rm -f "$LUMEN_AUTH_SUCCESS"' EXIT
      ${swayPackage}/bin/sway --unsupported-gpu --config ${swayConfig} || true
      if [[ -f "$LUMEN_AUTH_SUCCESS" ]]; then exit 0; fi
      exec ${pkgs.tuigreet}/bin/tuigreet --time --remember --asterisks --cmd ${swayPackage}/bin/sway
    '';
  };
  greeterConfig = (pkgs.formats.toml {}).generate "greetd.toml" {
    default_session = { command = "${greeter}/bin/lumen-greeter"; user = "greeter"; };
    terminal.vt = 1;
  };
  activateLogin = pkgs.writeShellApplication {
    name = "lumen-auth-activate-login";
    runtimeInputs = [ pkgs.coreutils pkgs.systemd ];
    text = ''
      if [[ $EUID -ne 0 ]]; then
        echo "Run this command with administrator privileges." >&2
        exit 1
      fi
      install -d -m 0755 /run/systemd/system/greetd.service.d
      cat > /run/systemd/system/greetd.service.d/90-lumen-auth.conf <<'CONF'
      [Service]
      ExecStart=
      ExecStart=${pkgs.greetd}/bin/greetd --config ${greeterConfig}
      CONF
      systemctl daemon-reload
      echo "Login screen configured for the next greetd restart."
      if [[ ''${1:-} == --restart ]]; then
        systemctl restart greetd
      fi
    '';
  };
in { inherit lock greeter assets fontConfig activateLogin; }
