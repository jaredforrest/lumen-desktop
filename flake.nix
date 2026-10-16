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
      runtime = import ./tests/desktop/runtime.nix { inherit pkgs; };
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
        character-viewer = import ./res/character-viewer { inherit pkgs; };
        localsend = import ./packages/localsend-desktop-style.nix { inherit pkgs; };
        test-runtime = runtime;
      };
      checks.${system} = {
        source-contracts = pkgs.runCommand "lumen-source-contracts" { nativeBuildInputs = [ pkgs.python3 ]; } ''
          python3 ${./tests/project/source_contracts.py} ${self}
          touch $out
        '';
        backend-units = pkgs.runCommand "lumen-backend-units" {
          nativeBuildInputs = [ python pkgs.dbus ];
        } ''
          export HOME=$TMPDIR/home
          mkdir -p "$HOME"
          export GI_TYPELIB_PATH=${pkgs.lib.makeSearchPath "lib/girepository-1.0" [ pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ]}
          export PYTHONDONTWRITEBYTECODE=1
          for test in ${self}/tests/desktop/test_*.py; do
            case "$test" in
              *test_nix_settings.py|*test_app_launch_qml.py|*test_dock_model_qml.py) continue ;;
            esac
            dbus-run-session --config-file=${pkgs.dbus}/share/dbus-1/session.conf -- ${python}/bin/python3 "$test"
          done
          touch $out
        '';
      };
      devShells.${system}.default = pkgs.mkShell {
        packages = [ runtime settings session.shellLauncher pkgs.nix pkgs.git pkgs.dbus pkgs.quickshell pkgs.sway pkgs.grim pkgs.wtype pkgs.gitleaks ];
        shellHook = ''
          export PYTHONDONTWRITEBYTECODE=1
          echo "Lumen development shell. See CONTRIBUTING.md for isolated test commands."
        '';
      };
    };
}
