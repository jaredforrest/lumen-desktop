{
  description = "Lumen desktop shell, session services and compositor integration";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/39ad350a0602fa0a58a544344e3e9187526ea45c";
  inputs.compositor-packages.url = "github:NixOS/nixpkgs/e7439b6b14ad3cc35d05608ebca9bce01a25f5f8";
  outputs = { self, nixpkgs, compositor-packages }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; config.allowUnfree = true; };
      compositorPkgs = import compositor-packages { inherit system; };
      quickshell = import ./packages/quickshell-runtime.nix { inherit pkgs; };
      settings = import ./packages/settings-adapter.nix { inherit pkgs; };
      session = import ./packages/session.nix { inherit pkgs; settingsPackage = settings; };
      profileBridge = import ./packages/lumen-native-profile.nix { inherit pkgs; nixSettings = settings; };
      office = import ./packages/craft-office.nix { inherit pkgs profileBridge; };
      runtime = import ./tests/desktop/runtime.nix { inherit pkgs; };
      testBusConfig = pkgs.writeText "lumen-test-session.conf" ''
        <busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>
      '';
      privateBus = pkgs.writeShellScriptBin "dbus-run-session" ''
        for argument in "$@"; do
          case "$argument" in
            --config-file=*) exec ${pkgs.dbus}/bin/dbus-run-session "$@" ;;
          esac
        done
        exec ${pkgs.dbus}/bin/dbus-run-session --config-file=${testBusConfig} "$@"
      '';
      python = (import ./packages/python-runtime.nix { inherit pkgs; }).withPackages (p: [ p.pygobject3 p.dbus-python p.astral p.icalendar p.recurring-ical-events p.pillow ]);
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
        desktop-action = session.desktopAction;
        reader-command = session.readerCommand;
        test-reader-command = import ./packages/reader-commands.nix { inherit pkgs; enabled = true; };
        app-launcher = session.appLauncher;
        settings-adapter = settings;
        authorization-agent = import ./packages/lumen-authorization-agent.nix { inherit pkgs; };
        compositor = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = true; };
        compositor-basic = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = false; };
        utilities = import ./packages/utilities.nix { inherit pkgs; };
        character-viewer = import ./res/character-viewer { inherit pkgs; };
        localsend = import ./packages/localsend-desktop-style.nix { inherit pkgs; };
        test-runtime = runtime;
        test-network-provider = pkgs.gvfs;
        test-capture-pipewire = pkgs.pipewire;
        test-screen-reader-atspi = import ./tests/desktop/screen_reader_atspi.nix { inherit pkgs; };
        test-screen-reader = import ./tests/desktop/screen_reader_package.nix { inherit pkgs; };
        test-screen-reader-tools = pkgs.symlinkJoin { name = "screen-reader-test-tools"; paths = [ pkgs.glib.dev pkgs.util-linux ]; };
        test-speech-source = pkgs.speechd.src;
        test-speech-dispatcher = import ./tests/desktop/speech_dispatcher_package.nix { inherit pkgs; };
        test-session-lock = import ./tests/desktop/session_lock_driver.nix { inherit pkgs; };
        clipboard-observer = import ./packages/clipboard-observer.nix { inherit pkgs; };
        test-clipboard-producer = import ./tests/desktop/clipboard_producer.nix { inherit pkgs; };
        test-window-relationships = import ./tests/desktop/window_relationship_client.nix { inherit pkgs; };
        test-constraint-client = import ./tests/desktop/constraint_client.nix { inherit pkgs; };
        test-typography = import ./tests/desktop/typography_probe.nix { inherit pkgs; };
        test-office-tools = pkgs.poppler-utils;
        test-application-software-driver = pkgs.mesa;
        shell-runtime = quickshell;
        test-physical-keyboard = import ./tests/desktop/physical_keyboard_driver.nix { inherit pkgs; };
        test-pointer = import ./tests/desktop/pointer_driver.nix { inherit pkgs; };
        inherit (office) wordcraft gridcraft deckcraft;
        profile-bridge = profileBridge;
        prism = import ./packages/prism-nix-profile.nix { inherit pkgs profileBridge; };
      };
      checks.${system} = {
        shortcut-registry = import ./tests/project/shortcuts.nix { inherit nixpkgs system; };
        javascript-models = pkgs.runCommand "lumen-javascript-models" { nativeBuildInputs = [ pkgs.nodejs ]; } ''
          cd ${self}
          for test in ${self}/tests/desktop/test_*.cjs; do
            node "$test"
          done
          touch "$out"
        '';
        authorization-appearance = import ./tests/auth/polkit-appearance.nix { inherit pkgs; };
        asset-overrides = let
          icons = ./tests/project/fixtures/icons;
          mark = ./tests/project/fixtures/mark.svg;
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
        font-configuration = import ./tests/project/font-configuration.nix { inherit pkgs; };
        consumer-boundary = assert import ./tests/project/boundary.nix { inherit nixpkgs system; }; pkgs.writeText "lumen-consumer-boundary" "passed";
        source-contracts = pkgs.runCommand "lumen-source-contracts" { nativeBuildInputs = [ pkgs.python3 ]; } ''
          python3 ${./tests/project/source_contracts.py} ${self}
          touch $out
        '';
        backend-units = pkgs.runCommand "lumen-backend-units" {
          nativeBuildInputs = [ pkgs.nodejs privateBus python quickshell session.shellLauncher pkgs.dbus pkgs.git pkgs.glib pkgs.procps pkgs.coreutils pkgs.util-linux ];
        } ''
          export HOME=$TMPDIR/home
          mkdir -p "$HOME"
          export GI_TYPELIB_PATH=${pkgs.lib.makeSearchPath "lib/girepository-1.0" [ pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ]}
          export PYTHONDONTWRITEBYTECODE=1
          export PYTHONWARNINGS=error
          export TZDIR=${pkgs.tzdata}/share/zoneinfo
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
        packages = [ pkgs.nodejs privateBus runtime settings pkgs.nix pkgs.git pkgs.dbus quickshell pkgs.sway pkgs.grim pkgs.wtype pkgs.foot pkgs.imagemagick pkgs.gitleaks pkgs.util-linux ];
        shellHook = ''
          export PYTHONDONTWRITEBYTECODE=1
          export __EGL_VENDOR_LIBRARY_FILENAMES=${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json
          export LUMEN_TEST_SETTINGS_COMMAND=${settings}/bin/lumen-nix-settings
          echo "Lumen development shell. See CONTRIBUTING.md for isolated test commands."
        '';
      };
    };
}
