{
  description = "Lumen desktop shell, session services and compositor integration";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/39ad350a0602fa0a58a544344e3e9187526ea45c";
  inputs.compositor-packages.url = "github:NixOS/nixpkgs/e7439b6b14ad3cc35d05608ebca9bce01a25f5f8";
  outputs = { self, nixpkgs, compositor-packages }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; config.allowUnfree = true; };
      compositorPkgs = import compositor-packages { inherit system; };
      settings = import ./packages/settings-adapter.nix { inherit pkgs; };
      session = import ./packages/session.nix { inherit pkgs; settingsPackage = settings; };
      profileBridge = import ./packages/lumen-native-profile.nix { inherit pkgs; nixSettings = settings; };
      office = import ./packages/craft-office.nix { inherit pkgs profileBridge; };
      runtime = import ./tests/desktop/runtime.nix { inherit pkgs; };
      privateBus = pkgs.writeShellScriptBin "dbus-run-session" ''
        exec ${pkgs.dbus}/bin/dbus-run-session --config-file=${pkgs.dbus}/share/dbus-1/session.conf "$@"
      '';
      python = pkgs.python3.withPackages (p: [ p.pygobject3 p.dbus-python p.astral p.icalendar p.recurring-ical-events p.pillow ]);
    in {
      nixosModules.default = import ./modules/desktop.nix;
      lib = {
        source = self;
        mkSession = import ./packages/session.nix;
        settingsSchema = import ./packages/desktop-settings-schema.nix;
      };
      packages.${system} = {
        default = session.shellLauncher;
        shell = session.shellLauncher;
        settings-adapter = settings;
        compositor = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = true; };
        compositor-basic = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = false; };
        utilities = import ./packages/utilities.nix { inherit pkgs; };
        character-viewer = import ./res/character-viewer { inherit pkgs; };
        localsend = import ./packages/localsend-desktop-style.nix { inherit pkgs; };
        test-runtime = runtime;
        inherit (office) wordcraft gridcraft deckcraft;
        profile-bridge = profileBridge;
        prism = import ./packages/prism-nix-profile.nix { inherit pkgs profileBridge; };
      };
      checks.${system} = {
        asset-overrides = let
          icons = pkgs.runCommand "test-consumer-icons" {} ''
            mkdir -p "$out"
            echo '<svg xmlns="http://www.w3.org/2000/svg"><title>Consumer override</title></svg>' > "$out/files.svg"
          '';
          mark = pkgs.writeText "test-consumer-mark.svg" '<svg xmlns="http://www.w3.org/2000/svg"><title>Consumer mark</title></svg>';
          wallpaper = ./res/lumen-desktop/shell/wallpaper.png;
          assets = import ./packages/assets.nix { inherit pkgs icons wallpaper; brandMark = mark; };
        in pkgs.runCommand "lumen-asset-overrides" {} ''
          cmp ${icons}/files.svg ${assets}/shell/reference-icons/files.svg
          cmp ${mark} ${assets}/shell/icons/lumen.svg
          cmp ${wallpaper} ${assets}/shell/wallpaper.png
          test -s ${assets}/shell/reference-icons/calendar.svg
          test -s ${assets}/auth/lock.qml
          touch "$out"
        '';
        consumer-boundary = assert import ./tests/project/boundary.nix { inherit nixpkgs system; }; pkgs.writeText "lumen-consumer-boundary" "passed";
        source-contracts = pkgs.runCommand "lumen-source-contracts" { nativeBuildInputs = [ pkgs.python3 ]; } ''
          python3 ${./tests/project/source_contracts.py} ${self}
          touch $out
        '';
        backend-units = pkgs.runCommand "lumen-backend-units" {
          nativeBuildInputs = [ privateBus python pkgs.dbus pkgs.git pkgs.glib pkgs.procps pkgs.coreutils pkgs.util-linux ];
        } ''
          export HOME=$TMPDIR/home
          mkdir -p "$HOME"
          export GI_TYPELIB_PATH=${pkgs.lib.makeSearchPath "lib/girepository-1.0" [ pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ]}
          export PYTHONDONTWRITEBYTECODE=1
          for test in ${self}/tests/desktop/test_*.py; do
            case "$test" in
              *test_nix_settings.py|*test_app_launch_qml.py|*test_dock_model_qml.py) continue ;;
            esac
            dbus-run-session -- ${python}/bin/python3 "$test"
          done
          touch $out
        '';
      };
      devShells.${system}.default = pkgs.mkShell {
        packages = [ privateBus runtime settings pkgs.nix pkgs.git pkgs.dbus pkgs.quickshell pkgs.sway pkgs.grim pkgs.wtype pkgs.gitleaks ];
        shellHook = ''
          export PYTHONDONTWRITEBYTECODE=1
          export LUMEN_TEST_SETTINGS_COMMAND=${settings}/bin/lumen-nix-settings
          echo "Lumen development shell. See CONTRIBUTING.md for isolated test commands."
        '';
      };
    };
}
