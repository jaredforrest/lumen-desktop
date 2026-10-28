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
      auth = import ./packages/lumen-auth.nix { inherit pkgs; };
      profileBridge = import ./packages/lumen-native-profile.nix { inherit pkgs; nixSettings = settings; };
      office = import ./packages/craft-office.nix { inherit pkgs profileBridge; };
      testBusConfig = pkgs.writeText "lumen-test-session.conf" ''
        <busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>
      '';
      privateBus = pkgs.writeTextFile {
        name = "lumen-private-dbus-run-session";
        destination = "/bin/dbus-run-session";
        executable = true;
        text = ''
          #!${pkgs.nushell}/bin/nu
          def --wrapped main [...args: string] {
            if ($args | any {|argument| $argument | str starts-with "--config-file=" }) {
              exec ${pkgs.dbus}/bin/dbus-run-session ...$args
            }
            exec ${pkgs.dbus}/bin/dbus-run-session --config-file=${testBusConfig} ...$args
          }
        '';
      };
      browserPipeChild = import ./packages/browser-pipe-child.nix { inherit pkgs; };
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
        desktop-state = session.desktopState;
        appearance-runtime = pkgs.callPackage ./packages/appearance-rust.nix { };
        network-locations = session.networkLocations;
        network-connectivity = pkgs.callPackage ./packages/network-connectivity.nix { };
        desktop-action = session.desktopAction;
        reader-command = session.readerCommand;
        test-reader-command = import ./packages/reader-commands.nix { inherit pkgs; enabled = true; };
        test-reader-commands-provider-fixture = import ./tests/desktop/reader-commands-provider-fixture { inherit pkgs; };
        app-launcher = session.appLauncher;
        launch-app = session.launchApp;
        open-folder = session.openFolder;
        settings-adapter = settings;
        authorization-agent = import ./packages/lumen-authorization-agent.nix { inherit pkgs; };
        global-menu-runtime = session.menuRuntime;
        compositor = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = true; };
        compositor-basic = import ./packages/sway-window-controls.nix { pkgs = compositorPkgs; effects = false; };
        utilities = import ./packages/utilities.nix { inherit pkgs; };
        file-jobs-rust = import ./packages/file-jobs-rust.nix { inherit pkgs; };
        ci-native-namespace-policy = import ./packages/ci-native-namespace-policy.nix { inherit pkgs; };
        office-capability-records = import ./packages/office-capability-records.nix { inherit pkgs; };
        browser-pipe-child = browserPipeChild;
        file-tags = import ./packages/file-tags.nix { inherit pkgs; };
        file-surfaces-rust-wip = import ./packages/file-surfaces-rust-wip.nix { inherit pkgs; };
        file-watch = import ./packages/file-watch.nix { inherit pkgs; };
        character-viewer = import ./res/character-viewer { inherit pkgs; };
        test-network-provider = pkgs.gvfs;
        test-network-provider-fixture = import ./tests/desktop/network-provider-fixture { inherit pkgs; };
        test-capture-pipewire = pkgs.pipewire;
        test-capture-observer = pkgs.callPackage ./packages/capture-observation.nix { };
        test-capture-protocol-fixture = import ./tests/desktop/capture-protocol-rust { inherit pkgs; };
        test-dialog-lifetime-fixture = import ./tests/desktop/dialog-lifetime-rust { inherit pkgs; };
        test-hot-corners-fixture = import ./tests/desktop/hot-corners-rust { inherit pkgs; };
        test-dock-matrix-fixture = import ./tests/desktop/dock-matrix-rust { inherit pkgs; };
        test-paired-picker-fixture = import ./tests/desktop/paired-picker-rust { inherit pkgs; };
        test-paired-layouts-fixture = import ./tests/desktop/paired-layouts-rust { inherit pkgs; };
        test-edge-layouts-fixture = import ./tests/desktop/edge-layouts-rust { inherit pkgs; };
        test-window-relationships-fixture = import ./tests/desktop/window-relationships-rust { inherit pkgs; };
        test-xwayland-relationships-fixture = import ./tests/desktop/xwayland-relationships-rust { inherit pkgs; };
        test-window-layout-menu-fixture = import ./tests/desktop/window-layout-menu-rust { inherit pkgs; };
        test-window-overview-fixture = import ./tests/desktop/window-overview-rust { inherit pkgs; };
        test-live-appearance-fixture = import ./tests/desktop/live-appearance-rust { inherit pkgs; };
        test-installed-smoke-fixture = import ./tests/desktop/smoke-rust { inherit pkgs; };
        test-status-clock-fixture = import ./tests/desktop/status-clock-rust { inherit pkgs; };
        test-input-settings-fixture = import ./tests/desktop/input-settings-rust { inherit pkgs; };
        test-wallpaper-settings-fixture = import ./tests/desktop/wallpaper-settings-rust { inherit pkgs; };
        test-screen-saver-fixture = import ./tests/desktop/screen-saver-rust { inherit pkgs; };
        test-calendar-widget-fixture = import ./tests/desktop/calendar-widget-rust { inherit pkgs; };
        test-widget-visibility-fixture = import ./tests/desktop/widget-visibility-rust { inherit pkgs; };
        test-widget-catalogue-fixture = import ./tests/desktop/widget-catalogue-rust { inherit pkgs; };
        test-fullscreen-menu-fixture = import ./tests/desktop/fullscreen-menu-rust { inherit pkgs; };
        test-surface-matrix-fixture = import ./tests/desktop/surface-matrix-rust { inherit pkgs; };
        test-icon-render-fixture = import ./tests/desktop/icon-render-rust { inherit pkgs; };
        test-window-frames-fixture = import ./tests/desktop/window-frames-rust { inherit pkgs; };
        test-content-clipping-fixture = import ./tests/desktop/content-clipping-rust { inherit pkgs; };
        test-fixture-infrastructure = import ./tests/desktop/fixture-infrastructure-rust { inherit pkgs; };
        test-display-visual-fixtures = import ./tests/desktop/display-visual-rust { inherit pkgs; };
        test-office-workflows-fixture = import ./tests/desktop/office-workflows-rust { inherit pkgs; };
        test-auth-render-fixture = import ./tests/desktop/auth-render-rust { inherit pkgs; };
        test-native-input-fixtures = import ./tests/desktop/native-input-rust { inherit pkgs; };
        test-preview-input-fixtures = import ./tests/desktop/preview-input-rust { inherit pkgs; };
        test-material-hierarchy-fixture = import ./tests/desktop/material-hierarchy-rust { inherit pkgs; };
        test-capture-native-fixture = import ./tests/desktop/capture-native-rust { inherit pkgs; };
        test-search-native-fixture = pkgs.callPackage ./tests/desktop/search-native-rust/default.nix {};
        test-browser-workflows-fixture = import ./tests/desktop/browser-workflows-rust { inherit pkgs; };
        test-preferred-zoom-fixture = import ./tests/desktop/preferred-zoom-rust { inherit pkgs; };
        test-accessibility-workflows-fixture = import ./tests/desktop/accessibility-workflows-rust { inherit pkgs; };
        test-accessibility-protocol-fixture = import ./tests/desktop/accessibility-protocol-rust { inherit pkgs; };
        test-gi-binding-native-fixture = import ./tests/desktop/gi-binding-native-rust { inherit pkgs; };
        test-shortcut-hints-fixture = import ./tests/desktop/shortcut-hints-rust { inherit pkgs; };
        test-batch-rename-fixture = import ./tests/desktop/batch-rename-rust { inherit pkgs; };
        test-trash-confirmation-fixture = import ./tests/desktop/trash-confirmation-rust { inherit pkgs; };
        test-files-layout-fixture = import ./tests/desktop/files-layout-rust { inherit pkgs; };
        test-appearance-controls-fixture = import ./tests/desktop/appearance-controls-rust { inherit pkgs; };
        test-desktop-surface-fixture = import ./tests/desktop/desktop-surface-rust { inherit pkgs; };
        test-dock-interaction-fixture = import ./tests/desktop/dock-interaction-rust { inherit pkgs; };
        test-files-share-menu-fixture = import ./tests/desktop/files-share-menu-rust { inherit pkgs; };
        test-screen-reader-atspi = import ./tests/desktop/screen_reader_atspi.nix { inherit pkgs; };
        test-screen-reader = import ./tests/desktop/screen_reader_package.nix { inherit pkgs; };
        test-screen-reader-tools = pkgs.symlinkJoin { name = "screen-reader-test-tools"; paths = [ pkgs.glib.dev pkgs.util-linux ]; };
        test-speech-source = pkgs.speechd.src;
        test-speech-dispatcher = import ./tests/desktop/speech_dispatcher_package.nix { inherit pkgs; };
        test-session-lock = import ./tests/desktop/session_lock_driver.nix { inherit pkgs; };
        clipboard-observer = import ./packages/clipboard-observer.nix { inherit pkgs; };
        test-clipboard-producer = import ./tests/desktop/clipboard_producer.nix { inherit pkgs; };
        test-window-relationships = import ./tests/desktop/window_relationship_client.nix { inherit pkgs; };
        test-xwayland-server = compositorPkgs.xwayland;
        test-xwayland-relationships = import ./tests/desktop/xwayland_relationship_client.nix { inherit pkgs; };
        test-constraint-client = import ./tests/desktop/constraint_client.nix { inherit pkgs; };
        test-typography = import ./tests/desktop/typography_probe.nix { inherit pkgs; };
        test-calendar-qa = import ./tests/desktop/calendar-qa { inherit pkgs; };
        test-calendar-runtime = session.personalServices.calendarRuntime;
        test-fixture-providers = import ./tests/desktop/provider-isolation { inherit pkgs; };
        test-typography-corpus = import ./tests/desktop/typography-corpus { inherit pkgs; };
        test-capture-observation-shim = import ./tests/desktop/capture-observation-shim { inherit pkgs; };
        test-application-isolation = import ./tests/desktop/application-isolation { inherit pkgs; };
        test-speech-transport-fixture = import ./tests/desktop/speech-transport-rust { inherit pkgs; };
        test-force-quit-native = import ./tests/desktop/force-quit-native { inherit pkgs; };
        test-input-pane-fixture = import ./tests/desktop/input-pane-rust { inherit pkgs; };
        test-transport-frame-fixture = import ./tests/desktop/transport-frame-rust { inherit pkgs; };
        test-portal-permission-provider-fixture = import ./tests/desktop/portal-permission-provider-rust { inherit pkgs; };
        test-portal-pane-fixture = import ./tests/desktop/portal-pane-rust { inherit pkgs; };
        test-portal-permissions-native-fixture = import ./tests/desktop/portal-permissions-native-rust { inherit pkgs; };
        test-mpris-mock-fixture = import ./tests/desktop/mpris-mock-rust { inherit pkgs; };
        test-menu-exporter-fixture = import ./tests/desktop/fixtures/menu-exporter-rust { inherit pkgs; };
        test-menu-native-fixture = import ./tests/desktop/menu-native-rust { inherit pkgs; };
        test-lifecycle-native-fixture = import ./tests/desktop/lifecycle-native-rust { inherit pkgs; };
        test-media-control-fixture = import ./tests/desktop/media-control-rust { inherit pkgs; };
        test-system-popover-fixture = import ./tests/desktop/system-popover-rust { inherit pkgs; };
        test-owned-window-context-fixture = import ./tests/desktop/owned-window-context-rust { inherit pkgs; };
        test-search-history-fixture = import ./tests/desktop/search-history-rust { inherit pkgs; };
        test-consumer-fit-fixture = import ./tests/desktop/consumer-fit-rust { inherit pkgs; };
        test-settings-search-fixture = import ./tests/desktop/settings-search-rust { inherit pkgs; };
        test-native-groups-fixture = import ./tests/desktop/native-groups-rust { inherit pkgs; };
        test-switcher-fixture = import ./tests/desktop/switcher-rust { inherit pkgs; };
        test-spaces-placement-fixture = import ./tests/desktop/spaces-placement-rust { inherit pkgs; };
        test-notification-native-fixture = import ./tests/desktop/notification-rust { inherit pkgs; };
        test-desktop-policy-fixture = import ./tests/desktop/desktop-policy-rust { inherit pkgs; };
        test-text-sizing-fixture = import ./tests/desktop/text-sizing-rust { inherit pkgs; };
        test-power-provider-fixture = import ./tests/desktop/power-provider-fixture { inherit pkgs; };
        test-network-connectivity-provider-fixture = import ./tests/desktop/network-connectivity-provider-fixture { inherit pkgs; };
        test-effects-software-render = import ./tests/desktop/effects-software-render { inherit pkgs; };
        test-office-tools = pkgs.poppler-utils;
        test-office-dialog-tools = pkgs.writeText "lumen-office-dialog-tools.json" (builtins.toJSON {
          portal = "${pkgs.xdg-desktop-portal}/libexec/xdg-desktop-portal";
          backend = "${pkgs.xdg-desktop-portal-gtk}/libexec/xdg-desktop-portal-gtk";
          backendData = "${pkgs.xdg-desktop-portal-gtk}/share";
          iconData = [ "${pkgs.adwaita-icon-theme}/share" "${pkgs.hicolor-icon-theme}/share" ];
          fontConfig = (import ./packages/font-configuration.nix { inherit pkgs; }) {
            fontDirectories = [ "${pkgs.dejavu_fonts}/share/fonts" ];
          };
          bus = "${pkgs.dbus}/bin/dbus-daemon";
        });
        test-offscreen-font-config = (import ./packages/font-configuration.nix { inherit pkgs; }) {
          fontDirectories = [ "${pkgs.dejavu_fonts}/share/fonts" ];
        };
        test-application-software-driver = pkgs.mesa;
        shell-runtime = quickshell;
        test-physical-keyboard = import ./tests/desktop/physical_keyboard_driver.nix { inherit pkgs; };
        test-application-font = pkgs.callPackage ./packages/interlude.nix {};
        test-pointer = import ./tests/desktop/pointer_driver.nix { inherit pkgs; };
        test-click-pointer = import ./tests/desktop/click_pointer.nix { inherit pkgs; };
        test-drag-pointer = import ./tests/desktop/drag_pointer.nix { inherit pkgs; };
        inherit (office) wordcraft gridcraft deckcraft;
        profile-bridge = profileBridge;
        profile-audit = import ./packages/lumen-profile-audit.nix { inherit pkgs; };
        asar-patch = import ./packages/asar-patch.nix { inherit pkgs; };
        prism = import ./packages/prism-nix-profile.nix { inherit pkgs profileBridge; };
      };
      checks.${system} = {
        fixture-providers = import ./tests/desktop/provider-isolation { inherit pkgs; };
        capture-observation-shim = import ./tests/desktop/capture-observation-shim { inherit pkgs; };
        gi-deprecation-rust = import ./tests/desktop/gi-deprecation-rust { inherit pkgs; };
        application-isolation = import ./tests/desktop/application-isolation { inherit pkgs; };
        file-jobs-rust = import ./packages/file-jobs-rust.nix { inherit pkgs; };
        ci-native-namespace-policy = import ./packages/ci-native-namespace-policy.nix { inherit pkgs; };
        office-capability-records = import ./packages/office-capability-records.nix { inherit pkgs; };
        file-tags = import ./packages/file-tags.nix { inherit pkgs; };
        file-surfaces-rust-wip = import ./packages/file-surfaces-rust-wip.nix { inherit pkgs; };
        file-watch = import ./packages/file-watch.nix { inherit pkgs; };
        launch-app = session.launchApp;
        open-folder = session.openFolder;
        shortcut-registry = import ./tests/project/shortcuts.nix { inherit nixpkgs system; };
        shortcut-hints = let
          defaults = import ./tests/project/shortcuts.nix { inherit nixpkgs system; bundle = true; };
          rebound = import ./tests/project/shortcuts.nix { inherit nixpkgs system; bundle = true; rebind = { from = "Mod4+Control+f"; to = "Mod4+Control+x"; }; };
          removed = import ./tests/project/shortcuts.nix { inherit nixpkgs system; extraBindings."Mod4+Control+f" = null; };
          replaced = import ./tests/project/shortcuts.nix { inherit nixpkgs system; extraBindings."Mod4+Control+f" = "exec printf consumer"; };
        in builtins.derivation {
          name = "lumen-shortcut-hint-fixture";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/project/shortcut_hints.nu}" "${defaults}" "${rebound}" "${removed}" "${replaced}" ];
        };
        offscreen-qml = builtins.derivation {
          name = "lumen-offscreen-qml";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/project/offscreen_qml.nu}" "${self}" ];
          PATH = pkgs.lib.makeBinPath [ pkgs.rustc pkgs.stdenv.cc pkgs.quickshell pkgs.coreutils privateBus pkgs.dbus (pkgs.callPackage ./packages/appearance-rust.nix {}) (pkgs.callPackage ./packages/shortcuts.nix {}) (pkgs.callPackage ./packages/desktop-layout.nix {}) ];
          LUMEN_TEST_TRUE = "${pkgs.coreutils}/bin/true";
          QML_IMPORT_PATH = "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml";
          QT_PLUGIN_PATH = "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins";
          LUMEN_FILES_BIN = "${pkgs.callPackage ./packages/file-surfaces-rust-wip.nix {}}/bin/lumen-files-rust-wip";
          LUMEN_FILE_JOBS_BIN = "${pkgs.callPackage ./packages/file-jobs-rust.nix {}}/bin/lumen-file-jobs";
          LUMEN_FILE_TAGS_BIN = "${pkgs.callPackage ./packages/file-tags.nix {}}/bin/lumen-file-tags";
          TZDIR = "${pkgs.tzdata}/share/zoneinfo";
        };
        nu-launchers = builtins.derivation {
          name = "lumen-nu-launchers";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [
            "${./tests/project/nu_launchers.nu}"
            "${session.hotCorners}/bin/lumen-hot-corners"
            "${session.spaces}/bin/lumen-spaces"
            "${session.windowOverview}/bin/lumen-window-overview"
            "${session.wallpaperPreferences}/bin/lumen-wallpaper-preferences"
            "${session.inputPreferences}/bin/lumen-input-preferences"
            "${session.displayLayout}/bin/lumen-display-layout"
            "${session.trashActions}/bin/lumen-trash-actions"
            "${session.fileOperations}/bin/lumen-file-operations"
            "${session.applicationLifecycle}/bin/lumen-application-lifecycle"
            "${session.applicationControl}/bin/lumen-application-control"
            "${session.fileAlias}/bin/lumen-file-alias"
            "${session.dockDrop}/bin/lumen-dock-drop"
            "${session.windowGeometry}/bin/lumen-window-geometry"
            "${session.clipboardHistory}/bin/lumen-clipboard-history"
            "${session.shellIpc}/bin/lumen-shell-ipc"
            "${session.desktopState}/bin/lumen-desktop-state"
            "${session.notifications}/bin/lumen-notifications"
            "${session.filesLauncher}/bin/lumen-files"
            "${session.remoteDesktopLauncher}/bin/lumen-remote-desktop"
            "${session.calendarLauncher}/bin/lumen-calendar-app"
            "${session.settingsLauncher}/bin/lumen-system-settings"
            "${session.audioKey}/bin/lumen-audio-key"
            "${session.desktopAction}/bin/lumen-desktop-action"
            "${session.appLauncher}/bin/lumen-app-launcher"
            "${session.readerCommand}/bin/lumen-reader-command"
            "${session.appearance}/bin/lumen-appearance"
            "${session.nixSettings}/bin/lumen-nix-settings"
            "${session.personalServices}/bin/lumen-google-calendar-setup"
            "${session.personalServices}/bin/lumen-calendar"
            "${session.personalServices}/bin/lumen-google-calendar-sync"
            "${session.personalServices}/bin/lumen-icloud-setup"
            "${privateBus}/bin/dbus-run-session"
            "${session.desktopDoctor}/bin/lumen-desktop-doctor"
            "${profileBridge}/bin/lumen-native-profile"
            "${auth.greeterClient}"
            "${auth.greeter}/bin/lumen-greeter"
            "${auth.activateLogin}/bin/lumen-auth-activate-login"
            "${auth.lock}/bin/lumen-lock"
            "${pkgs.callPackage ./packages/file-preview.nix { }}/bin/lumen-file-preview"
            "${pkgs.callPackage ./packages/search-history.nix { nixSettings = settings; }}/bin/lumen-search-history"
            "${pkgs.callPackage ./packages/menu-bridge.nix { }}/bin/lumen-menu-bridge"
            "${session.systemControls}/bin/lumen-system-controls"
            "${pkgs.callPackage ./packages/privacy-status.nix { }}/bin/lumen-privacy-status"
            "${pkgs.callPackage ./packages/menu-visibility.nix { }}/bin/lumen-menu-visibility"
            "${pkgs.callPackage ./packages/desktop-files.nix { }}/bin/lumen-desktop-files"
            "${session.focusWindow}/bin/lumen-focus-window"
            "${pkgs.callPackage ./packages/bluetooth-power.nix { }}/bin/lumen-bluetooth-power"
          ];
          PATH = pkgs.lib.makeBinPath [ pkgs.nushell pkgs.coreutils ];
        };
        greeter-wrapper = let
          fakeSway = pkgs.writeTextFile {
            name = "fixture-sway";
            destination = "/bin/sway";
            executable = true;
            text = ''
              #!${pkgs.nushell}/bin/nu
              def --wrapped main [...args: string] {
                if ($env.LUMEN_TEST_SWAY_SUCCESS? | default "") == "1" {
                  ^${pkgs.coreutils}/bin/touch $env.LUMEN_AUTH_SUCCESS
                  exit 0
                }
                exit 1
              }
            '';
          };
          fakeTuigreet = pkgs.writeTextFile {
            name = "fixture-tuigreet";
            destination = "/bin/tuigreet";
            executable = true;
            text = ''
              #!${pkgs.nushell}/bin/nu
              def --wrapped main [...args: string] {
                $args | to json | save --force $env.LUMEN_TEST_LOG
              }
            '';
          };
          fixtureAuth = import ./packages/lumen-auth.nix { pkgs = pkgs // { tuigreet = fakeTuigreet; }; swayPackage = fakeSway; };
        in builtins.derivation {
          name = "lumen-greeter-wrapper-fixture";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/auth/greeter_wrapper.nu}" "${fixtureAuth.greeter}/bin/lumen-greeter" ];
          PATH = pkgs.lib.makeBinPath [ pkgs.nushell pkgs.coreutils ];
        };
        lock-launcher = let
          launcher = import ./packages/lock-launcher.nix { inherit pkgs; };
          fakeQuickshell = pkgs.writeTextFile {
            name = "fixture-lock-quickshell";
            destination = "/bin/quickshell";
            executable = true;
            text = ''
              #!${pkgs.nushell}/bin/nu
              def --wrapped main [...args: string] {
                if "--daemonize" in $args {
                  if $env.LUMEN_LOCK_FIXTURE_MODE == "start-failed" { exit 1 }
                  $args | to json | save --force $env.LUMEN_LOCK_FIXTURE_START
                  exit 0
                }
                if "secure" in $args {
                  print ($env.LUMEN_LOCK_FIXTURE_MODE == "covered")
                  exit 0
                }
                exit 2
              }
            '';
          };
          fakeSwaylock = pkgs.writeTextFile {
            name = "fixture-swaylock";
            destination = "/bin/swaylock";
            executable = true;
            text = ''
              #!${pkgs.nushell}/bin/nu
              def --wrapped main [...args: string] {
                $args | to json | save --force $env.LUMEN_LOCK_FIXTURE_FALLBACK
              }
            '';
          };
        in builtins.derivation {
          name = "lumen-lock-launcher-fixture";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/auth/lock_launcher.nu}" "${launcher}/bin/lumen-lock-launcher" "${fakeQuickshell}/bin/quickshell" "${fakeSwaylock}/bin/swaylock" ];
          PATH = pkgs.lib.makeBinPath [ pkgs.nushell pkgs.coreutils ];
        };
        private-bus = builtins.derivation {
          name = "lumen-private-bus-fixture";
          inherit system;
          builder = "${privateBus}/bin/dbus-run-session";
          args = [ "--" "${pkgs.nushell}/bin/nu" "${./tests/project/private_bus.nu}" ];
          PATH = pkgs.lib.makeBinPath [ pkgs.dbus pkgs.nushell pkgs.coreutils ];
        };
        authorization-appearance = import ./tests/auth/polkit-appearance.nix { inherit pkgs; };
        asset-overrides = let
          icons = ./tests/project/fixtures/icons;
          mark = ./tests/project/fixtures/mark.svg;
          wallpaper = ./res/lumen-desktop/shell/wallpaper.png;
          assets = import ./packages/assets.nix { inherit pkgs icons wallpaper; brandMark = mark; };
        in builtins.derivation {
          name = "lumen-asset-overrides";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/project/asset_overrides.nu}" "${icons}" "${mark}" "${wallpaper}" "${assets}" ];
          PATH = pkgs.lib.makeBinPath [ pkgs.diffutils pkgs.coreutils ];
        };
        font-configuration = import ./tests/project/font-configuration.nix { inherit pkgs; };
        consumer-boundary = assert import ./tests/project/boundary.nix { inherit nixpkgs system; }; pkgs.writeText "lumen-consumer-boundary" "passed";
        source-contracts = builtins.derivation {
          name = "lumen-source-contracts";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/project/source_contracts.nu}" "${self}" ];
        };
        transport-frame-fixture = import ./tests/desktop/transport-frame-rust { inherit pkgs; };
        backend-units = let
          inputs = [ pkgs.bash pkgs.nodejs privateBus browserPipeChild quickshell (import ./tests/desktop/input-pane-rust { inherit pkgs; }) (import ./tests/desktop/portal-permission-provider-rust { inherit pkgs; }) (import ./tests/desktop/portal-pane-rust { inherit pkgs; }) (pkgs.callPackage ./packages/file-tags.nix { }) (pkgs.callPackage ./packages/file-watch.nix { }) (pkgs.callPackage ./packages/file-preview.nix { }) (pkgs.callPackage ./packages/file-jobs-rust.nix { }) session.shellLauncher session.launchApp session.openFolder (pkgs.callPackage ./packages/hot-corners.nix { }) (pkgs.callPackage ./packages/desktop-layout.nix { }) (pkgs.callPackage ./packages/remote-desktop.nix { }) (pkgs.callPackage ./packages/window-pair.nix { }) (pkgs.callPackage ./packages/capture-observation.nix { }) (pkgs.callPackage ./packages/portal-permissions.nix { }) (pkgs.callPackage ./packages/reader-commands-rust.nix { }) (pkgs.callPackage ./packages/notifications-runtime.nix { }) (pkgs.callPackage ./packages/airdrop-consent.nix { }) (pkgs.callPackage ./packages/airdrop-ui.nix { }) (pkgs.callPackage ./packages/wallpaper-settings.nix { }) (pkgs.callPackage ./packages/input-settings.nix { }) (pkgs.callPackage ./packages/screen-saver.nix { }) (pkgs.callPackage ./packages/display-settings.nix { }) (pkgs.callPackage ./packages/file-alias.nix { }) (pkgs.callPackage ./packages/dock-drop.nix { }) (pkgs.callPackage ./packages/shortcuts.nix { }) (pkgs.callPackage ./packages/appearance-rust.nix { }) (pkgs.callPackage ./packages/power-status.nix { }) (pkgs.callPackage ./packages/network-connectivity.nix { }) (pkgs.callPackage ./packages/native-window-preferences-rust.nix { }) (pkgs.callPackage ./packages/search-history.nix { nixSettings = settings; }) (pkgs.callPackage ./packages/vpn-control.nix { }) (pkgs.callPackage ./packages/shell-ipc.nix { }) (pkgs.callPackage ./packages/nearby-sharing.nix { }) pkgs.dbus pkgs.git pkgs.glib pkgs.procps pkgs.coreutils pkgs.util-linux ];
        in builtins.derivation {
          name = "lumen-backend-units";
          inherit system;
          builder = "${pkgs.nushell}/bin/nu";
          args = [ "${./tests/project/backend_units.nu}" "${self}" ];
          PATH = pkgs.lib.makeBinPath ([ pkgs.nushell ] ++ inputs);
          LUMEN_SHELL_IPC_BIN = "${pkgs.callPackage ./packages/shell-ipc.nix { }}/bin/lumen-shell-ipc";
        };
      };
      devShells.${system}.default = pkgs.mkShell {
        packages = [ pkgs.nodejs pkgs.nushell pkgs.bubblewrap (import ./tests/desktop/portal-permission-provider-rust { inherit pkgs; }) (import ./tests/desktop/portal-pane-rust { inherit pkgs; }) (pkgs.callPackage ./tests/desktop/provider-isolation { inherit pkgs; }) (import ./tests/desktop/capture-observation-shim { inherit pkgs; }) (import ./tests/desktop/application-isolation { inherit pkgs; }) (pkgs.callPackage ./packages/ci-native-namespace-policy.nix { }) (pkgs.callPackage ./packages/office-capability-records.nix { }) privateBus browserPipeChild settings (pkgs.callPackage ./packages/file-preview.nix { }) (pkgs.callPackage ./packages/shell-ipc.nix { }) (pkgs.callPackage ./packages/power-status.nix { }) (pkgs.callPackage ./packages/portal-permissions.nix { }) pkgs.nix pkgs.git pkgs.dbus quickshell pkgs.sway pkgs.grim pkgs.wtype pkgs.foot pkgs.imagemagick pkgs.gitleaks pkgs.util-linux ];
        __EGL_VENDOR_LIBRARY_FILENAMES = "${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json";
        LUMEN_TEST_SETTINGS_COMMAND = "${settings}/bin/lumen-nix-settings";
      };
    };
}
