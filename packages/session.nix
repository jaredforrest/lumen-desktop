{ pkgs, settingsPackage, user ? "user", displayName ? user, homeDirectory ? "/home/${user}", timeZone ? "UTC", swayPackage ? pkgs.sway, rofiPackage ? pkgs.rofi, assets ? ../res/lumen-desktop, emojiFamily ? "Noto Color Emoji", clipboardRetentionSeconds ? 3600, clipboardMaxItems ? 100, networkProviderPackage ? null, readerCommandsEnabled ? false, nearbySharingProvider ? null, nearbySharingLabel ? "Nearby Sharing" }:
let
  lib = pkgs.lib;
  quickshell = import ./quickshell-runtime.nix { inherit pkgs; };
  desktopTimeZone = timeZone;
  readerCommand = import ./reader-commands.nix { inherit pkgs assets; enabled = readerCommandsEnabled; };
  auth = import ./lumen-auth.nix { inherit pkgs user displayName assets; swayPackage = swayPackage; };
    nixSettings = settingsPackage;
    appearance = import ../packages/lumen-appearance.nix { inherit pkgs nixSettings searchHistory; };
    qtTheme = import ../packages/lumen-qt-theme.nix { inherit pkgs; };
    qtTheme5 = import ../packages/lumen-qt-theme.nix { inherit pkgs; qt6 = false; };
    palette = builtins.fromJSON (builtins.readFile ../res/lumen-desktop/appearance/palette.json);
    menuRuntime = import ../packages/global-menu-runtime.nix { inherit pkgs; };
    menuVisibility = pkgs.callPackage ./menu-visibility.nix { };
    personalServices = import ../packages/lumen-personal-services.nix { inherit pkgs; timeZone = desktopTimeZone; };
    fileClipboard = import ../packages/file-clipboard.nix { inherit pkgs; };
    fileClipboardClient = pkgs.callPackage ./file-clipboard-client.nix { };
    bluetoothPower = pkgs.callPackage ./bluetooth-power.nix { };
    bluetoothPair = pkgs.callPackage ./bluetooth-pair.nix { };
    desktopFiles = pkgs.callPackage ./desktop-files.nix { };
    filePreviewRust = pkgs.callPackage ./file-preview.nix { };
    fileSurfacesRust = pkgs.callPackage ./file-surfaces-rust-wip.nix { };
    batchRenameRust = pkgs.callPackage ./batch-rename.nix { };
    fileJobsRust = pkgs.callPackage ./file-jobs-rust.nix { };
    fileTagsRust = pkgs.callPackage ./file-tags.nix { };
    fileWatchRust = pkgs.callPackage ./file-watch.nix { };
    privacyStatus = pkgs.callPackage ./privacy-status.nix { };
    powerStatus = pkgs.callPackage ./power-status.nix { };
    networkConnectivity = pkgs.callPackage ./network-connectivity.nix { };
    systemControls = pkgs.callPackage ./system-controls.nix { inherit bluetoothPower powerStatus networkConnectivity; };
    contextTools = pkgs.callPackage ./context-tools.nix { };
    textTools = pkgs.callPackage ./text-tools.nix { inherit nixSettings contextTools; };
    searchHistory = pkgs.callPackage ./search-history.nix { inherit nixSettings; };
    vpnControl = pkgs.callPackage ./vpn-control.nix { };
    fileAliasRust = pkgs.callPackage ./file-alias.nix { };
    dockDropRust = pkgs.callPackage ./dock-drop.nix { };
    displaySettingsRust = pkgs.callPackage ./display-settings.nix { };
    screenSaverRust = pkgs.callPackage ./screen-saver.nix { };
    inputSettingsRust = pkgs.callPackage ./input-settings.nix { };
    wallpaperSettingsRust = pkgs.callPackage ./wallpaper-settings.nix { };
    notificationRuntime = pkgs.callPackage ./notifications-runtime.nix { };
    hotCornersRust = pkgs.callPackage ./hot-corners.nix { };
    desktopLayoutRust = pkgs.callPackage ./desktop-layout.nix { };
    remoteDesktopRust = pkgs.callPackage ./remote-desktop.nix { };
    windowPairRust = pkgs.callPackage ./window-pair.nix { };
    captureObservationRust = pkgs.callPackage ./capture-observation.nix { };
    portalPermissionsRust = pkgs.callPackage ./portal-permissions.nix { };
    groupControlRust = pkgs.callPackage ./group-control.nix { };
    applicationControlRust = pkgs.callPackage ./application-control.nix { };
    applicationLifecycleRust = pkgs.callPackage ./application-lifecycle.nix { };
    spacesRust = pkgs.callPackage ./spaces.nix { };
    stateRust = pkgs.callPackage ./desktop-state.nix { };
    nuLauncher = { name, runtimeInputs ? [ ], body }: pkgs.writeTextFile {
      inherit name;
      destination = "/bin/${name}";
      executable = true;
      text = "#!${pkgs.nushell}/bin/nu\n" + ''
        def --wrapped main [...args] {
          ${lib.optionalString (runtimeInputs != [ ]) ''$env.PATH = ((${builtins.toJSON (lib.makeBinPath runtimeInputs)} | split row ':') ++ $env.PATH)''}
          ${body}
        }
      '';
    };
    nuExec = { name, command, runtimeInputs ? [ ], arguments ? "...$args", setup ? "" }: nuLauncher {
      inherit name runtimeInputs;
      body = ''
        ${setup}
        exec ${command} ${arguments}
      '';
    };
    hotCorners = nuExec { name = "lumen-hot-corners"; command = "${hotCornersRust}/bin/lumen-hot-corners"; runtimeInputs = [ pkgs.sway desktopAction appLauncher auth.lock ]; };
    spaces = nuExec { name = "lumen-spaces"; command = "${spacesRust}/bin/lumen-spaces"; runtimeInputs = [ pkgs.sway ]; };
    windowOverview = nuExec { name = "lumen-window-overview"; command = "${spacesRust}/bin/lumen-window-overview"; };
    wallpaperPreferences = nuExec {
      name = "lumen-wallpaper-preferences";
      command = "${wallpaperSettingsRust}/bin/lumen-wallpaper-settings";
      runtimeInputs = [ pkgs.sway nixSettings ];
      arguments = "watch";
      setup = ''$env.LUMEN_WALLPAPER_FALLBACK = "${assets}/shell/wallpaper.png"'';
    };
    inputPreferences = nuExec { name = "lumen-input-preferences"; command = "${inputSettingsRust}/bin/lumen-input-settings"; runtimeInputs = [ nixSettings ]; arguments = "watch"; };
    displayLayout = nuExec { name = "lumen-display-layout"; command = "${displaySettingsRust}/bin/lumen-display-settings"; runtimeInputs = [ pkgs.sway nixSettings launchApp ]; arguments = "watch"; };
    networkLocations = import ./lumen-network-locations.nix { inherit pkgs; providerPackage = networkProviderPackage; };
    trashActions = nuExec { name = "lumen-trash-actions"; command = "${fileJobsRust}/bin/lumen-file-jobs"; arguments = "trash-client ...$args"; };
    fileOperations = nuExec { name = "lumen-file-operations"; command = "${fileJobsRust}/bin/lumen-file-jobs"; arguments = "serve"; };
    applicationLifecycle = nuExec { name = "lumen-application-lifecycle"; command = "${applicationLifecycleRust}/bin/lumen-application-lifecycle"; runtimeInputs = [ pkgs.sway ]; };
    applicationControl = nuExec { name = "lumen-application-control"; command = "${applicationControlRust}/bin/lumen-application-control"; runtimeInputs = [ pkgs.sway ]; };
    fileAlias = nuLauncher {
      name = "lumen-file-alias";
      body = ''
        if ($env.LUMEN_LAUNCH_SCOPES? | default "") == "1" {
          exec ${launchApp}/bin/lumen-launch-app -- ${fileAliasRust}/bin/lumen-file-alias --gio ${pkgs.glib}/bin/gio ...$args
        }
        exec ${fileAliasRust}/bin/lumen-file-alias --gio ${pkgs.glib}/bin/gio ...$args
      '';
    };
    dockDrop = nuLauncher {
      name = "lumen-dock-drop";
      body = ''
        if ($env.LUMEN_LAUNCH_SCOPES? | default "") == "1" {
          exec ${launchApp}/bin/lumen-launch-app -- ${dockDropRust}/bin/lumen-dock-drop ...$args
        }
        exec ${dockDropRust}/bin/lumen-dock-drop ...$args
      '';
    };
    focusWindow = pkgs.callPackage ./focus-window.nix { };
    windowGeometryRust = pkgs.callPackage ./window-geometry.nix { };
    windowGeometry = nuExec { name = "lumen-window-geometry"; command = "${windowGeometryRust}/bin/lumen-window-geometry"; runtimeInputs = [ pkgs.sway ]; };
    clipboardObserver = import ./clipboard-observer.nix { inherit pkgs; };
    clipboardHistoryRust = pkgs.callPackage ./clipboard-history.nix { };
    clipboardHistory = nuExec {
      name = "lumen-clipboard-history";
      command = "${clipboardHistoryRust}/bin/lumen-clipboard-history";
      runtimeInputs = [ clipboardObserver pkgs.wl-clipboard pkgs.systemd pkgs.coreutils ];
      setup = ''
        $env.LUMEN_CLIPBOARD_RETENTION_SECONDS = ($env.LUMEN_CLIPBOARD_RETENTION_SECONDS? | default "${toString clipboardRetentionSeconds}")
        $env.LUMEN_CLIPBOARD_MAX_ITEMS = ($env.LUMEN_CLIPBOARD_MAX_ITEMS? | default "${toString clipboardMaxItems}")
      '';
    };
    shellIpcRust = pkgs.callPackage ./shell-ipc.nix { };
    shellIpc = nuExec {
      name = "lumen-shell-ipc";
      command = "${shellIpcRust}/bin/lumen-shell-ipc";
      runtimeInputs = [ pkgs.systemd quickshell ];
      arguments = "--systemctl ${pkgs.systemd}/bin/systemctl --quickshell ${quickshell}/bin/quickshell -- ...$args";
    };
    desktopState = nuExec { name = "lumen-desktop-state"; command = "${stateRust}/bin/lumen-desktop-state"; runtimeInputs = [ pkgs.sway ]; };
    launchApp = import ./launch-app.nix { inherit pkgs; };
    openFolder = import ./open-folder.nix { inherit pkgs; };
    nearbySharing = pkgs.callPackage ./nearby-sharing.nix { };
    shellLauncher = nuLauncher {
      name = "lumen-desktop-shell";
      # Desktop entries launched through Rofi inherit this service's PATH.
      # Entries such as ChatGPT use `env` from coreutils in their Exec command.
      runtimeInputs = [ filePreviewRust fileSurfacesRust batchRenameRust contextTools nearbySharing desktopLayoutRust remoteDesktopRust windowPairRust captureObservationRust portalPermissionsRust pkgs.remmina pkgs.fontconfig pkgs.hyprpicker personalServices pkgs.rembg pkgs.whisper-cpp pkgs.pipewire (pkgs.tesseract.override { enableLanguages = [ "eng" ]; }) (pkgs.aspellWithDicts (d: [ d.en ])) pkgs.wordnet pkgs.espeak-ng pkgs.wtype pkgs.poppler-utils pkgs.ffmpeg-headless pkgs.util-linux fileClipboard fileClipboardClient bluetoothPower bluetoothPair desktopFiles privacyStatus powerStatus networkConnectivity systemControls textTools searchHistory vpnControl pkgs.wl-clipboard pkgs.glib pkgs.udisks2 launchApp pkgs.ddcutil pkgs.blueman pkgs.coreutils pkgs.jq pkgs.sway focusWindow menuVisibility windowGeometry quickshell pkgs.foot pkgs.wireplumber pkgs.pavucontrol
        hotCorners spaces windowOverview networkLocations trashActions fileTagsRust fileWatchRust clipboardHistory groupControlRust desktopState shellIpc dockDrop displaySettingsRust screenSaverRust inputSettingsRust wallpaperSettingsRust applicationControl applicationLifecycle fileAlias auth.lock pkgs.swaylock pkgs.xdg-utils pkgs.trash-cli appearance nixSettings rofiPackage ];
      body = ''
        # The systemd service does not inherit the interactive login PATH.
        # Desktop entries may name installed commands without absolute paths.
        $env.PATH = ($env.PATH ++ ["/etc/profiles/per-user/${user}/bin" "/run/current-system/sw/bin" "/run/wrappers/bin"])
        $env.LUMEN_SEARCH_ROOTS = ${builtins.toJSON (builtins.toJSON (map (name: "${homeDirectory}/${name}") [ "Desktop" "Documents" "Downloads" "Pictures" "Music" "Videos" "data" ]))}
        $env.LUMEN_NEARBY_SHARING_LABEL = ${builtins.toJSON nearbySharingLabel}
        $env.LUMEN_NEARBY_SHARING_PROVIDER = ${builtins.toJSON (if nearbySharingProvider == null then "" else "${nearbySharingProvider}/bin/lumen-nearby-provider")}
        $env.LUMEN_NOTIFICATION_HELPER = "${notificationRuntime}/bin/lumen-notifications"
        $env.LUMEN_DESKTOP_ASSETS = "${assets}"
        $env.LUMEN_LAUNCH_SCOPES = "1"
        $env.LUMEN_NATIVE_WINDOW_CONTROLS = "0"
        $env.LUMEN_COMPOSITOR_EFFECTS = "0"
        $env.LUMEN_UNIFIED_OWNED_HEADERS = "1"
        $env.LUMEN_NATIVE_WINDOW_ACTIONS = "0"
        if ($env.SWAYSOCK? | default "") != "" {
          let version = (^swaymsg -t get_version -r | complete)
          if $version.exit_code == 0 {
            let capabilities = (try { $version.stdout | from json } catch { {} })
            if ($capabilities.native_window_actions? | default false) == true { $env.LUMEN_NATIVE_WINDOW_ACTIONS = "1" }
            if ($capabilities.compositor_effects? | default false) == true { $env.LUMEN_COMPOSITOR_EFFECTS = "1" }
            if ($capabilities.native_window_controls? | default false) == true { $env.LUMEN_NATIVE_WINDOW_CONTROLS = "1" }
          }
        }
        $env.U2NET_HOME = "${pkgs.linkFarm "desktop-subject-model" [{ name = "u2netp.onnx"; path = pkgs.fetchurl {
          url = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx";
          hash = "sha256-MJyEaSWN2nQnk9zg6+qObdOTF0+Jk0cz7MixTHb03dg=";
        }; }]}"
        $env.LUMEN_DICTATION_MODEL = "${pkgs.fetchurl {
          url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
          hash = "sha256-oDd5yG3zMjB19eeWyyzlAp8A7Ihp7uP9+4l6/jbG0AI=";
        }}"
        $env.LUMEN_FILE_SERVICE = "1"
        $env.LUMEN_FILE_JOBS_BIN = "${fileJobsRust}/bin/lumen-file-jobs"
        $env.LUMEN_FILES_BIN = "${fileSurfacesRust}/bin/lumen-files-rust-wip"
        $env.LUMEN_SEARCH_BIN = "${fileSurfacesRust}/bin/lumen-search-rust-wip"
        $env.LUMEN_OWNED_PREVIEW_BIN = "${fileSurfacesRust}/bin/lumen-owned-preview-rust-wip"
        $env.LUMEN_FILE_PREVIEW_BIN = "${filePreviewRust}/bin/lumen-file-preview"
        $env.LUMEN_FILE_TAGS_BIN = "${fileTagsRust}/bin/lumen-file-tags"
        $env.LUMEN_BATCH_RENAME_BIN = "${batchRenameRust}/bin/lumen-batch-rename"
        $env.LUMEN_FILE_WATCH_BIN = "${fileWatchRust}/bin/lumen-file-watch"
        $env.GLOBAL_MENU_ENABLED = "1"
        $env.LUMEN_CONTEXT_TOOLS = "${contextTools}/bin/lumen-context-tools"
        $env.GLOBAL_MENU_BRIDGE = "${menuRuntime}/share/global-menu/bridge"
        $env.QT_QPA_PLATFORMTHEME = "gnome"
        $env.QT_PLUGIN_PATH = $"${qtTheme}/${pkgs.qt6.qtbase.qtPluginPrefix}:${qtTheme5}/${pkgs.qt5.qtbase.qtPluginPrefix}:${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins:($env.QT_PLUGIN_PATH? | default "")"
        $env.QML_IMPORT_PATH = $"${pkgs.qt6.qtmultimedia}/lib/qt-6/qml:($env.QML_IMPORT_PATH? | default "")"
        hide-env -i NO_AT_BRIDGE GTK_A11Y
        $env.QT_LINUX_ACCESSIBILITY_ALWAYS_ON = "1"
        $env.QT_QPA_PLATFORM = "wayland"
        $env.QSG_RHI_BACKEND = "opengl"
        if ($args.0? | default "") == "--print-env0" {
          exec ${pkgs.coreutils}/bin/env -0
        }
        exec ${quickshell}/bin/quickshell --no-duplicate --path ${assets}/shell
      '';
    };
    audioKey = nuLauncher {
      name = "lumen-audio-key";
      body = ''
        let action = ($args.0? | default "")
        if $action not-in [up down mute] { exit 2 }
        let ipc = (^${shellIpc}/bin/lumen-shell-ipc call desktop audioKey $action | complete)
        if $ipc.exit_code == 0 { exit 0 }
        match $action {
          up => { exec ${pkgs.wireplumber}/bin/wpctl set-volume -l 1 @DEFAULT_AUDIO_SINK@ 5%+ }
          down => { exec ${pkgs.wireplumber}/bin/wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- }
          mute => { exec ${pkgs.wireplumber}/bin/wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle }
        }
      '';
    };
    mediaKey = pkgs.callPackage ./media-key.nix { };
    notifications = nuExec {
      name = "lumen-notifications";
      command = "${notificationRuntime}/bin/lumen-notifications";
      setup = ''$env.LUMEN_NIX_SETTINGS_BIN = "${nixSettings}/bin/lumen-nix-settings"'';
    };
    menuCommand = textTools;
    desktopDoctor = import ./doctor.nix { inherit pkgs emojiFamily; };
    desktopAction = nuLauncher {
      name = "lumen-desktop-action";
      body = ''
        let action = ($args.0? | default "")
        match $action {
          show-desktop => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop showDesktop }
          window-layout => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop windowLayout }
          window-next => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop cycleWindow next }
          window-previous => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop cycleWindow previous }
          app-switcher => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher next }
          app-switcher-reverse => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher previous }
          app-switcher-commit => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop appSwitcher commit }
          window-groups => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop windowGroups }
          overview => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop overview false }
          app-windows => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop overview true }
          text-tools => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop textTools }
          recognize-image => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop recognizeImage ($args.1? | default "") }
          notifications-open => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre open }
          notifications-close => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre close }
          notifications-toggle => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop notificationCentre toggle }
          controls => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop menu Controls }
          dock => { exec ${shellIpc}/bin/lumen-shell-ipc call desktop focusDock }
          _ => { exit 2 }
        }
      '';
    };
    appLauncher = nuLauncher {
      name = "lumen-app-launcher";
      body = ''
        let ipc = (^${shellIpc}/bin/lumen-shell-ipc call desktop launcher | complete)
        if $ipc.exit_code == 0 { exit 0 }
        exec ${rofiPackage}/bin/rofi -show drun
      '';
    };
    filesLauncher = nuExec { name = "lumen-files"; command = "${openFolder}/bin/lumen-open-folder"; arguments = "--ipc-command ${shellIpc}/bin/lumen-shell-ipc -- ...$args"; };
    remoteDesktopLauncher = nuExec { name = "lumen-remote-desktop"; command = "${shellIpc}/bin/lumen-shell-ipc"; arguments = "call desktop remoteDesktop"; };
    calendarLauncher = nuExec { name = "lumen-calendar-app"; command = "${shellIpc}/bin/lumen-shell-ipc"; arguments = "call desktop calendar"; };
    settingsLauncher = nuExec { name = "lumen-system-settings"; command = "${shellIpc}/bin/lumen-shell-ipc"; arguments = "call desktop settings \"\""; };
in { inherit readerCommand hotCorners spaces windowOverview wallpaperPreferences inputPreferences networkLocations quickshell nixSettings appearance qtTheme qtTheme5 palette menuRuntime personalServices fileClipboard displayLayout trashActions fileOperations focusWindow windowGeometry dockDrop applicationControl applicationLifecycle fileAlias desktopState shellIpc clipboardHistory launchApp openFolder shellLauncher audioKey mediaKey notifications menuCommand desktopDoctor desktopAction appLauncher filesLauncher remoteDesktopLauncher calendarLauncher settingsLauncher systemControls; }
