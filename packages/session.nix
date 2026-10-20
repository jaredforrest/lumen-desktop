{ pkgs, settingsPackage, user ? "user", displayName ? user, homeDirectory ? "/home/${user}", timeZone ? "UTC", swayPackage ? pkgs.sway, rofiPackage ? pkgs.rofi, assets ? ../res/lumen-desktop, emojiFamily ? "Noto Color Emoji", clipboardRetentionSeconds ? 3600, clipboardMaxItems ? 100, networkProviderPackage ? null }:
let
  lib = pkgs.lib;
  quickshell = import ./quickshell-runtime.nix { inherit pkgs; };
  desktopTimeZone = timeZone;
  auth = import ./lumen-auth.nix { inherit pkgs user displayName assets; swayPackage = swayPackage; };
    nixSettings = settingsPackage;
    appearance = import ../packages/lumen-appearance.nix { inherit pkgs nixSettings; };
    qtTheme = import ../packages/lumen-qt-theme.nix { inherit pkgs; };
    qtTheme5 = import ../packages/lumen-qt-theme.nix { inherit pkgs; qt6 = false; };
    palette = builtins.fromJSON (builtins.readFile ../res/lumen-desktop/appearance/palette.json);
    menuRuntime = import ../packages/global-menu-runtime.nix { inherit pkgs; };
    personalServices = import ../packages/lumen-personal-services.nix { inherit pkgs; timeZone = desktopTimeZone; };
    fileClipboard = import ../packages/file-clipboard.nix { inherit pkgs; };
    hotCorners = pkgs.writeShellApplication {
      name = "lumen-hot-corners";
      runtimeInputs = [ pkgs.sway desktopAction appLauncher auth.lock ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/hot_corners.py "$@"
      '';
    };
    spaces = pkgs.writeShellApplication {
      name = "lumen-spaces";
      runtimeInputs = [ pkgs.sway ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/spaces.py "$@"
      '';
    };
    wallpaperPreferences = pkgs.writeShellApplication {
      name = "lumen-wallpaper-preferences";
      runtimeInputs = [ pkgs.sway nixSettings ];
      text = ''
        export LUMEN_WALLPAPER_FALLBACK=${assets}/shell/wallpaper.png
        exec ${menuRuntime}/bin/global-menu-python ${assets}/shell/wallpaper_settings.py watch
      '';
    };
    inputPreferences = pkgs.writeShellApplication {
      name = "lumen-input-preferences";
      runtimeInputs = [ nixSettings ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/shell/input_settings.py watch
      '';
    };
    displayLayout = pkgs.writeShellApplication {
      name = "lumen-display-layout";
      runtimeInputs = [ pkgs.sway nixSettings ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/shell/display_settings.py watch
      '';
    };
    networkLocations = import ./lumen-network-locations.nix { inherit pkgs menuRuntime assets; providerPackage = networkProviderPackage; };
    fileOperations = pkgs.writeShellApplication {
      name = "lumen-file-operations";
      runtimeInputs = [ pkgs.trash-cli ];
      text = ''
        exec ${menuRuntime}/bin/global-menu-python ${assets}/shell/file_jobs.py
      '';
    };
    applicationLifecycle = pkgs.writeShellApplication {
      name = "lumen-application-lifecycle";
      runtimeInputs = [ pkgs.sway ];
      text = ''
        exec ${menuRuntime}/bin/global-menu-python ${assets}/runtime/application_lifecycle.py "$@"
      '';
    };
    applicationControl = pkgs.writeShellApplication {
      name = "lumen-application-control";
      runtimeInputs = [ pkgs.sway ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/application_control.py "$@"
      '';
    };
    fileAlias = pkgs.writeShellScriptBin "lumen-file-alias" ''
      if [ "''${LUMEN_LAUNCH_SCOPES:-}" = 1 ]; then
        exec ${launchApp}/bin/lumen-launch-app -- ${menuRuntime}/bin/global-menu-python ${assets}/runtime/file_alias.py "$@"
      fi
      exec ${menuRuntime}/bin/global-menu-python ${assets}/runtime/file_alias.py "$@"
    '';
    dockDrop = pkgs.writeShellScriptBin "lumen-dock-drop" ''
      if [ "''${LUMEN_LAUNCH_SCOPES:-}" = 1 ]; then
        exec ${launchApp}/bin/lumen-launch-app -- ${menuRuntime}/bin/global-menu-python ${assets}/runtime/app_drop.py "$@"
      fi
      exec ${menuRuntime}/bin/global-menu-python ${assets}/runtime/app_drop.py "$@"
    '';
    focusWindow = pkgs.writeShellScriptBin "lumen-focus-window" ''
      export PATH=${lib.makeBinPath [ pkgs.sway ]}:"$PATH"
      exec ${pkgs.python3}/bin/python ${assets}/focus-window.py "$@"
    '';
    clipboardObserver = import ./clipboard-observer.nix { inherit pkgs; };
    clipboardHistory = pkgs.writeShellApplication {
      name = "lumen-clipboard-history";
      runtimeInputs = [ clipboardObserver pkgs.wl-clipboard pkgs.systemd ];
      text = ''
        export LUMEN_CLIPBOARD_RETENTION_SECONDS=${toString clipboardRetentionSeconds}
        export LUMEN_CLIPBOARD_MAX_ITEMS=${toString clipboardMaxItems}
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/clipboard_history.py "$@"
      '';
    };
    shellIpc = pkgs.writeShellApplication {
      name = "lumen-shell-ipc";
      runtimeInputs = [ pkgs.systemd quickshell ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/shell_ipc.py --systemctl ${pkgs.systemd}/bin/systemctl --quickshell ${quickshell}/bin/quickshell -- "$@"
      '';
    };
    desktopState = pkgs.writeShellApplication {
      name = "lumen-desktop-state";
      runtimeInputs = [ pkgs.sway ];
      text = ''
        exec ${pkgs.python3}/bin/python3 ${assets}/runtime/state.py "$@"
      '';
    };
    launchApp = pkgs.writeShellApplication {
      name = "lumen-launch-app";
      runtimeInputs = [ pkgs.python3 pkgs.systemd ];
      text = ''
        exec python3 ${assets}/launch-app.py "$@"
      '';
    };
    shellLauncher = pkgs.writeShellApplication {
      name = "lumen-desktop-shell";
      # Desktop entries launched through Rofi inherit this service's PATH.
      # Entries such as ChatGPT use `env` from coreutils in their Exec command.
      runtimeInputs = [ pkgs.remmina pkgs.fontconfig pkgs.hyprpicker personalServices pkgs.rembg pkgs.whisper-cpp pkgs.pipewire (pkgs.tesseract.override { enableLanguages = [ "eng" ]; }) (pkgs.aspellWithDicts (d: [ d.en ])) pkgs.wordnet pkgs.espeak-ng pkgs.wtype pkgs.poppler-utils pkgs.ffmpeg-headless pkgs.util-linux fileClipboard pkgs.wl-clipboard pkgs.glib pkgs.udisks2 launchApp pkgs.ddcutil pkgs.blueman pkgs.coreutils pkgs.jq pkgs.sway focusWindow quickshell pkgs.foot pkgs.wireplumber pkgs.pavucontrol
        hotCorners spaces networkLocations clipboardHistory desktopState shellIpc dockDrop applicationControl applicationLifecycle fileAlias auth.lock pkgs.swaylock pkgs.xdg-utils pkgs.trash-cli appearance nixSettings rofiPackage ];
      text = ''
        # The systemd service does not inherit the interactive login PATH.
        # Desktop entries may name installed commands without absolute paths.
        export PATH="$PATH:/etc/profiles/per-user/${user}/bin:/run/current-system/sw/bin:/run/wrappers/bin"
        export LUMEN_SEARCH_ROOTS=${lib.escapeShellArg (builtins.toJSON (map (name: "${homeDirectory}/${name}") [ "Desktop" "Documents" "Downloads" "Pictures" "Music" "Videos" "data" ]))}
        export LUMEN_NOTIFICATION_HELPER=${assets}/notifications/service.py
        export LUMEN_LAUNCH_SCOPES=1
        export LUMEN_NATIVE_WINDOW_CONTROLS=0
        export LUMEN_COMPOSITOR_EFFECTS=0
        export LUMEN_UNIFIED_OWNED_HEADERS=1
        export LUMEN_NATIVE_WINDOW_ACTIONS=0
        if [ -n "''${SWAYSOCK:-}" ] && swaymsg -t get_version -r | jq -e '.native_window_actions == true' >/dev/null; then
          export LUMEN_NATIVE_WINDOW_ACTIONS=1
        fi
        if [ -n "''${SWAYSOCK:-}" ] && swaymsg -t get_version -r | jq -e '.compositor_effects == true' >/dev/null; then
          export LUMEN_COMPOSITOR_EFFECTS=1
        fi
        if [ -n "''${SWAYSOCK:-}" ] && swaymsg -t get_version -r | jq -e '.native_window_controls == true' >/dev/null; then
          export LUMEN_NATIVE_WINDOW_CONTROLS=1
        fi
        export U2NET_HOME=${pkgs.linkFarm "desktop-subject-model" [{ name = "u2netp.onnx"; path = pkgs.fetchurl {
          url = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx";
          hash = "sha256-MJyEaSWN2nQnk9zg6+qObdOTF0+Jk0cz7MixTHb03dg=";
        }; }]}
        export LUMEN_DICTATION_MODEL=${pkgs.fetchurl {
          url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
          hash = "sha256-oDd5yG3zMjB19eeWyyzlAp8A7Ihp7uP9+4l6/jbG0AI=";
        }}
        export LUMEN_FILE_SERVICE=1
        export GLOBAL_MENU_ENABLED=1
        export GLOBAL_MENU_PYTHON=${menuRuntime}/bin/global-menu-python
        export GLOBAL_MENU_BRIDGE=${menuRuntime}/share/global-menu/bridge.py
        export QT_QPA_PLATFORMTHEME=gnome
        export QT_PLUGIN_PATH=${qtTheme}/${pkgs.qt6.qtbase.qtPluginPrefix}:${qtTheme5}/${pkgs.qt5.qtbase.qtPluginPrefix}:${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins:"''${QT_PLUGIN_PATH:-}"
        export QML_IMPORT_PATH=${pkgs.qt6.qtmultimedia}/lib/qt-6/qml:"''${QML_IMPORT_PATH:-}"
        unset NO_AT_BRIDGE GTK_A11Y
        export QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
        export QT_QPA_PLATFORM=wayland
        export QSG_RHI_BACKEND=opengl
        exec quickshell --no-duplicate --path ${assets}/shell
      '';
    };
    audioKey = pkgs.writeShellScriptBin "lumen-audio-key" ''
      case "$1" in up|down|mute) ;; *) exit 2 ;; esac
      if ${shellIpc}/bin/lumen-shell-ipc call desktop audioKey "$1"; then exit 0; fi
      case "$1" in
        up) exec ${pkgs.wireplumber}/bin/wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+ ;;
        down) exec ${pkgs.wireplumber}/bin/wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- ;;
        mute) exec ${pkgs.wireplumber}/bin/wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle ;;
      esac
    '';
    mediaKey = pkgs.writeShellScriptBin "lumen-media-key" ''
      exec ${menuRuntime}/bin/global-menu-python ${assets}/runtime/media_key.py "$@"
    '';
    notifications = pkgs.writeShellScriptBin "lumen-notifications" ''
      export PATH=${nixSettings}/bin:$PATH
      exec ${menuRuntime}/bin/global-menu-python ${assets}/notifications/service.py "$@"
    '';
    menuCommand = pkgs.writeShellScriptBin "lumen-menu-command" ''
      export PATH=${nixSettings}/bin:$PATH
      exec ${menuRuntime}/bin/global-menu-python ${assets}/shell/text_tools.py "$@"
    '';
    desktopDoctor = pkgs.writeShellApplication {
      name = "lumen-desktop-doctor";
      runtimeInputs = [ pkgs.python3 pkgs.systemd pkgs.fontconfig pkgs.glib ];
      text = ''
        export LUMEN_EMOJI_FONT_FAMILY=${lib.escapeShellArg emojiFamily}
        export GSETTINGS_SCHEMA_DIR=${pkgs.glib.getSchemaPath pkgs.gsettings-desktop-schemas}
        exec python3 ${assets}/doctor.py "$@"
      '';
    };
    desktopAction = pkgs.writeShellScriptBin "lumen-desktop-action" ''
      case "$1" in
        show-desktop) exec ${shellIpc}/bin/lumen-shell-ipc call desktop showDesktop ;;
        window-layout) exec ${shellIpc}/bin/lumen-shell-ipc call desktop windowLayout ;;
        window-next) exec ${shellIpc}/bin/lumen-shell-ipc call desktop cycleWindow next ;;
        window-previous) exec ${shellIpc}/bin/lumen-shell-ipc call desktop cycleWindow previous ;;
        app-switcher) exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher next ;;
        app-switcher-reverse) exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher previous ;;
        app-switcher-commit) exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher commit ;;
        window-groups) exec ${shellIpc}/bin/lumen-shell-ipc call desktop windowGroups ;;
        overview) exec ${shellIpc}/bin/lumen-shell-ipc call desktop overview false ;;
        app-windows) exec ${shellIpc}/bin/lumen-shell-ipc call desktop overview true ;;
        text-tools) exec ${shellIpc}/bin/lumen-shell-ipc call desktop textTools ;;
        recognize-image) exec ${shellIpc}/bin/lumen-shell-ipc call desktop recognizeImage "$2" ;;
        notifications-open) exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre open ;;
        notifications-close) exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre close ;;
        notifications-toggle) exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre toggle ;;
        controls) exec ${shellIpc}/bin/lumen-shell-ipc call desktop menu Controls ;;
        dock) exec ${shellIpc}/bin/lumen-shell-ipc call desktop focusDock ;;
        *) exit 2 ;;
      esac
    '';
    appLauncher = pkgs.writeShellScriptBin "lumen-app-launcher" ''
      ${shellIpc}/bin/lumen-shell-ipc call desktop launcher \
        || exec ${rofiPackage}/bin/rofi -show drun
    '';
    filesLauncher = pkgs.writeShellScriptBin "lumen-files" ''
      exec ${pkgs.python3}/bin/python3 ${assets}/open-folder.py --ipc-command ${shellIpc}/bin/lumen-shell-ipc -- "$@"
    '';
    remoteDesktopLauncher = pkgs.writeShellScriptBin "lumen-remote-desktop" ''
      exec ${shellIpc}/bin/lumen-shell-ipc call desktop remoteDesktop
    '';
    calendarLauncher = pkgs.writeShellScriptBin "lumen-calendar-app" ''
      exec ${shellIpc}/bin/lumen-shell-ipc call desktop calendar
    '';
    settingsLauncher = pkgs.writeShellScriptBin "lumen-system-settings" ''
      exec ${shellIpc}/bin/lumen-shell-ipc call desktop settings ""
    '';
in { inherit hotCorners spaces wallpaperPreferences inputPreferences networkLocations quickshell nixSettings appearance qtTheme qtTheme5 palette menuRuntime personalServices fileClipboard displayLayout fileOperations focusWindow dockDrop applicationControl applicationLifecycle fileAlias desktopState shellIpc clipboardHistory launchApp shellLauncher audioKey mediaKey notifications menuCommand desktopDoctor desktopAction appLauncher filesLauncher remoteDesktopLauncher calendarLauncher settingsLauncher; }
