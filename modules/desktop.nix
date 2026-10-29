# Desktop mechanisms and opt-in session integrations. Host policy belongs to consumers.
{ config, pkgs, lib, ... }:
let
  cfg = config.services.lumen-desktop;
  desktopUser = cfg.user;
  settingsPackage = cfg.settingsPackage;
  hostGvfsPackage = config.services.gvfs.package;
  hostSwayPackage = config.programs.sway.package;
  assets = import ../packages/assets.nix { inherit pkgs; inherit (cfg.appearance) icons wallpaper brandMark; };
  desktopTimeZone = config.time.timeZone;
  authorizationAgent = import ../packages/lumen-authorization-agent.nix { inherit pkgs; };
  auth = import ../packages/lumen-auth.nix { inherit pkgs lib assets; inherit (cfg.integrations.greeter) greeterExtraConfig allowUnsupportedGpu; user = desktopUser; displayName = cfg.displayName; swayPackage = config.programs.sway.package; };
in {
  options.services.lumen-desktop = {
    enable = lib.mkEnableOption "Lumen desktop session";
    user = lib.mkOption { type = lib.types.str; description = "Desktop login user."; };
    displayName = lib.mkOption { type = lib.types.str; default = cfg.user; description = "Name shown by the greeter and lock screen."; };
    settingsPackage = lib.mkOption { type = lib.types.package; description = "Host settings command implementing the settings JSON contract."; };
    shortcuts = {
      readerBindings = lib.mkOption {
        default = {};
        type = lib.types.attrsOf (lib.types.enum [ "where-am-i" "read-all" "stop" ]);
        description = "Explicit consumer key chords for the opt-in typed reader adapter. No keys are reserved by default.";
      };
      applicationBindings = lib.mkOption {
        default = [];
        description = "Selected application adapter shortcuts used for overlap validation; does not rewrite application input.";
        type = lib.types.listOf (lib.types.submodule {
          options = {
            id = lib.mkOption { type = lib.types.str; };
            action = lib.mkOption { type = lib.types.str; };
            chord = lib.mkOption { type = lib.types.str; };
            appId = lib.mkOption { type = lib.types.str; description = "Exact application identity for this adapter."; };
            mode = lib.mkOption { type = lib.types.str; default = "default"; };
            enabled = lib.mkOption { type = lib.types.bool; default = true; };
          };
        });
      };
      reservedBindings = lib.mkOption {
        default = [];
        description = "Consumer reservations, including security shortcuts. Any overlapping active binding fails validation.";
        type = lib.types.listOf (lib.types.submodule {
          options = {
            id = lib.mkOption { type = lib.types.str; };
            action = lib.mkOption { type = lib.types.str; };
            chord = lib.mkOption { type = lib.types.str; };
            mode = lib.mkOption { type = lib.types.str; default = "default"; };
          };
        });
      };
    };
    appearance = {
      icons = lib.mkOption { type = lib.types.nullOr lib.types.path; default = null; description = "Optional icon directory; supplied by the consumer, never bundled by this module."; };
      brandMark = lib.mkOption { type = lib.types.nullOr lib.types.path; default = null; };
      wallpaper = lib.mkOption { type = lib.types.path; default = ../res/lumen-desktop/shell/wallpaper.png; };
      emojiFamily = lib.mkOption { type = lib.types.str; default = "Noto Color Emoji"; };
    };
    integrations = {
      greeter = {
        enable = lib.mkEnableOption "Lumen greetd greeter";
        greeterExtraConfig = lib.mkOption { type = lib.types.lines; default = ""; };
        allowUnsupportedGpu = lib.mkOption { type = lib.types.bool; default = false; };
      };
      clipboard = {
        enable = lib.mkEnableOption "opt-in clipboard history integration";
        retentionSeconds = lib.mkOption { type = lib.types.nullOr (lib.types.ints.between 60 86400); default = null; description = "Consumer-selected ephemeral history lifetime; required when enabled."; };
        maxItems = lib.mkOption { type = lib.types.nullOr (lib.types.ints.between 1 1000); default = null; description = "Consumer-selected history bound; required when enabled."; };
      };
      nearbySharing = {
        providerPackage = lib.mkOption { type = lib.types.nullOr lib.types.package; default = null; description = "Optional consumer provider exposing bin/lumen-nearby-provider status and open. Does not install or enable a receiver service."; };
        label = lib.mkOption { type = lib.types.str; default = "Nearby Sharing"; description = "Consumer-selected sharing control label."; };
      };
      shellRecovery = {
        enableFallback = lib.mkOption { type = lib.types.bool; default = true; description = "Open an independent recovery terminal when the shell exhausts its restart limit."; };
        fallbackPackage = lib.mkOption { type = lib.types.nullOr lib.types.package; default = null; description = "Optional consumer package exposing bin/lumen-session-recovery. The default terminal only explains recovery and never restarts the session automatically."; };
      };
      authorization.enable = lib.mkEnableOption "session authorization dialog appearance integration";
      calendar.enable = lib.mkEnableOption "calendar account services";
      bluetooth.enable = lib.mkEnableOption "Bluetooth UI integration";
      networkLocations.enable = lib.mkEnableOption "network location providers supplied by the host GVfs service";
      accessibility.enable = lib.mkEnableOption "desktop accessibility bus";
      readerCommands.enable = lib.mkEnableOption "typed commands to an already running session reader";
      replaceExistingShell = lib.mkEnableOption "replacement of existing bar and notification services";
    };
  };
  config = lib.mkIf cfg.enable {
  assertions = [ {
    assertion = !cfg.integrations.clipboard.enable || (cfg.integrations.clipboard.retentionSeconds != null && cfg.integrations.clipboard.maxItems != null);
    message = "Clipboard integration requires an explicit consumer retentionSeconds and maxItems policy.";
  } {
    assertion = !cfg.integrations.networkLocations.enable || config.services.gvfs.enable;
    message = "Network locations require the consumer to enable services.gvfs explicitly.";
  } ];
  services.gnome.at-spi2-core.enable = lib.mkIf cfg.integrations.accessibility.enable true;
  services.gnome.gnome-online-accounts.enable = lib.mkIf cfg.integrations.calendar.enable true;
  services.gnome.evolution-data-server.enable = lib.mkIf cfg.integrations.calendar.enable true;
  environment.systemPackages = lib.optionals cfg.integrations.calendar.enable [ pkgs.gnome-online-accounts-gtk ]
    ++ lib.optionals cfg.integrations.greeter.enable [ auth.activateLogin ];
  services.blueman.enable = lib.mkIf cfg.integrations.bluetooth.enable true;
  services.greetd.settings.default_session.command = lib.mkIf cfg.integrations.greeter.enable
    (lib.mkDefault "${auth.greeter}/bin/lumen-greeter");
  # Never interrupt an active session when changing greeter packaging.
  systemd.services.greetd.restartIfChanged = lib.mkIf cfg.integrations.greeter.enable false;
  systemd.services.greetd.stopIfChanged = lib.mkIf cfg.integrations.greeter.enable false;

  home-manager.users.${desktopUser} = { config, lib, ... }:
  let
    session = import ../packages/session.nix {
      inherit pkgs settingsPackage assets;
      readerCommandsEnabled = cfg.integrations.readerCommands.enable;
      nearbySharingProvider = cfg.integrations.nearbySharing.providerPackage;
      nearbySharingLabel = cfg.integrations.nearbySharing.label;
      networkProviderPackage = if cfg.integrations.networkLocations.enable then hostGvfsPackage else null;
      emojiFamily = cfg.appearance.emojiFamily;
      clipboardRetentionSeconds = if cfg.integrations.clipboard.retentionSeconds == null then 3600 else cfg.integrations.clipboard.retentionSeconds;
      clipboardMaxItems = if cfg.integrations.clipboard.maxItems == null then 100 else cfg.integrations.clipboard.maxItems;
      user = config.home.username;
      displayName = cfg.displayName;
      homeDirectory = config.home.homeDirectory;
      timeZone = desktopTimeZone;
      swayPackage = hostSwayPackage;
      rofiPackage = config.programs.rofi.package;
    };
    inherit (session) readerCommand hotCorners spaces wallpaperPreferences inputPreferences networkLocations quickshell nixSettings appearance qtTheme qtTheme5 palette menuRuntime personalServices fileClipboard displayLayout fileOperations focusWindow desktopState clipboardHistory launchApp shellLauncher audioKey mediaKey notifications menuCommand desktopDoctor desktopAction appLauncher filesLauncher remoteDesktopLauncher calendarLauncher settingsLauncher;
    shellRecovery = if cfg.integrations.shellRecovery.fallbackPackage == null then import ../packages/session-recovery.nix { inherit pkgs; } else cfg.integrations.shellRecovery.fallbackPackage;
    readerBindings = lib.optionalAttrs cfg.integrations.readerCommands.enable (lib.mapAttrs (_: command:
      "exec ${readerCommand}/bin/lumen-reader-command ${command}") cfg.shortcuts.readerBindings);
    baseDesktopKeybindings = {
      "Mod4+Tab" = "exec ${desktopAction}/bin/lumen-desktop-action app-switcher";
      "Mod4+Shift+Tab" = "exec ${desktopAction}/bin/lumen-desktop-action app-switcher-reverse";
      "--release Super_L" = "exec ${desktopAction}/bin/lumen-desktop-action app-switcher-commit";
      "--release Super_R" = "exec ${desktopAction}/bin/lumen-desktop-action app-switcher-commit";
      "Control+Left" = "exec ${spaces}/bin/lumen-spaces switch previous";
      "Control+Right" = "exec ${spaces}/bin/lumen-spaces switch next";
      "Mod4+Control+g" = "exec ${desktopAction}/bin/lumen-desktop-action window-groups";
      "Control+Up" = "exec ${desktopAction}/bin/lumen-desktop-action overview";
      "Control+Down" = "exec ${desktopAction}/bin/lumen-desktop-action app-windows";
      "Mod4+Control+Shift+d" = "exec ${desktopAction}/bin/lumen-desktop-action dock";
      "Mod4+Control+Shift+n" = "exec ${desktopAction}/bin/lumen-desktop-action notifications-toggle";
      "Mod4+Control+Shift+c" = "exec ${desktopAction}/bin/lumen-desktop-action controls";
      "Mod4+Control+Shift+f" = "exec ${hostSwayPackage}/bin/swaymsg lumen_window fill";
      "Mod4+Control+d" = "exec ${desktopAction}/bin/lumen-desktop-action show-desktop";
      "Mod4+Control+l" = "exec ${desktopAction}/bin/lumen-desktop-action window-layout";
      "Mod4+Control+f" = "exec ${hostSwayPackage}/bin/swaymsg fullscreen toggle";
      "Mod4+grave" = "exec ${desktopAction}/bin/lumen-desktop-action window-next";
      "Mod4+Shift+grave" = "exec ${desktopAction}/bin/lumen-desktop-action window-previous";
      "Mod4+Mod1+t" = "exec ${desktopAction}/bin/lumen-desktop-action text-tools";
      "Mod4+n" = "exec ${desktopAction}/bin/lumen-desktop-action notifications-toggle";
      "Control+F3" = "exec ${desktopAction}/bin/lumen-desktop-action dock";
      "XF86AudioRaiseVolume" = "exec ${audioKey}/bin/lumen-audio-key up";
      "XF86AudioLowerVolume" = "exec ${audioKey}/bin/lumen-audio-key down";
      "XF86AudioMute" = "exec ${audioKey}/bin/lumen-audio-key mute";
      "XF86AudioPlay" = "exec ${mediaKey}/bin/lumen-media-key play-pause";
      "XF86AudioNext" = "exec ${mediaKey}/bin/lumen-media-key next";
      "XF86AudioPrev" = "exec ${mediaKey}/bin/lumen-media-key previous";
    } // builtins.listToAttrs (map (n: { name = "Mod4+Mod1+F${toString n}"; value = "exec ${menuCommand}/bin/lumen-menu-command slot ${toString n}"; }) (lib.range 1 12));
    desktopKeybindings = baseDesktopKeybindings // readerBindings;
    shortcutDefaults = (builtins.fromJSON (builtins.readFile ../res/lumen-desktop/runtime/shortcut-defaults.json)).bindings;
    effectiveShortcutBindings = lib.mapAttrsToList (key: command:
      let known = lib.findFirst (item: builtins.hasAttr item.chord baseDesktopKeybindings && command == baseDesktopKeybindings.${item.chord}) null shortcutDefaults;
          readerKey = lib.findFirst (name: command == readerBindings.${name}) null (builtins.attrNames readerBindings);
          owned = builtins.hasAttr key desktopKeybindings && command == desktopKeybindings.${key};
      in { id = "sway:" + key; chord = key; scope = "global";
        action = if readerKey != null then "reader." + cfg.shortcuts.readerBindings.${readerKey} else if known != null then known.action else "consumer.binding";
        owner = if owned then "desktop" else "consumer";
        fallback = if known != null then known.fallback else "Consumer-defined binding";
      }) (lib.filterAttrs (_: command: command != null) config.wayland.windowManager.sway.config.keybindings);
    shortcutRegistryInput = pkgs.writeText "lumen-shortcut-declarations.json" (builtins.toJSON {
      version = 1;
      bindings = effectiveShortcutBindings ++ map (item: item // { scope = "application"; owner = "application-adapter"; }) cfg.shortcuts.applicationBindings
        ++ map (item: item // { scope = "global"; owner = "reserved"; }) cfg.shortcuts.reservedBindings;
    });
    shortcutRegistry = pkgs.runCommand "lumen-effective-shortcuts.json" { nativeBuildInputs = [ pkgs.python3 ]; } ''
      python3 ${../res/lumen-desktop/runtime/shortcuts.py} ${../res/lumen-desktop/runtime/shortcut-defaults.json} > /dev/null
      python3 ${../res/lumen-desktop/runtime/shortcuts.py} ${shortcutRegistryInput} --output "$out"
    '';
  in {
    assertions = [{
      assertion = builtins.intersectAttrs readerBindings baseDesktopKeybindings == {};
      message = "Reader command bindings must not replace desktop shortcuts.";
    }];
    # Session UI only: the consumer retains all authentication policy.
    wayland.windowManager.sway.config.startup = lib.optionals cfg.integrations.authorization.enable [
      { command = "${authorizationAgent}/libexec/polkit-gnome-authentication-agent-1"; }
    ];
    # Own the theme declaratively, rather than keeping temporary config links.
    wayland.windowManager.sway.extraConfig = lib.mkAfter (builtins.readFile ../res/lumen-desktop/sway.conf + ''
      # libinput exposes three-finger swipes; two-finger edge gestures are not available in Sway.
      bindgesture swipe:3:left exec ${desktopAction}/bin/lumen-desktop-action notifications-open
      bindgesture swipe:3:right exec ${desktopAction}/bin/lumen-desktop-action notifications-close
    '');
    wayland.windowManager.sway.config.menu = lib.mkForce "${appLauncher}/bin/lumen-app-launcher";
    programs.rofi.theme = lib.mkForce "${config.xdg.stateHome}/lumen-desktop/launcher.rasi";
    programs.rofi.settings.display-drun = lib.mkForce "⌕";
    programs.rofi.settings.drun-display-format = lib.mkForce "{name}";
    gtk.theme = { name = lib.mkDefault "WhiteSur-Light"; package = lib.mkDefault pkgs.whitesur-gtk-theme; };
    gtk.gtk4.theme = config.gtk.theme;
    gtk.font = { name = "Interlude Variable"; size = 10; };
    # GTK reads both its own settings and the desktop portal's GSettings value.
    gtk.gtk3.extraConfig.gtk-decoration-layout = "close,minimize,maximize:";
    gtk.gtk4.extraConfig.gtk-decoration-layout = "close,minimize,maximize:";
    dconf.settings."org/gnome/desktop/wm/preferences".button-layout = "close,minimize,maximize:";
    dconf.settings."org/gnome/desktop/interface".toolkit-accessibility = true;
    dconf.settings."org/gnome/desktop/interface".font-name = "Interlude Variable 10";
    dconf.settings."org/gnome/desktop/interface".document-font-name = "Interlude Variable 10";
    dconf.settings."org/gnome/desktop/interface".color-scheme = lib.mkDefault "default";
    home.activation.lumenAppearance = lib.hm.dag.entryAfter [ "dconfSettings" ] ''
      $DRY_RUN_CMD ${appearance}/bin/lumen-appearance
    '';
    systemd.user.services.lumen-appearance = {
      Unit = { Description = "Scheduled desktop appearance"; After = [ "graphical-session.target" ]; PartOf = [ "graphical-session.target" ]; };
      Service = { Type = "oneshot"; ExecStart = "${appearance}/bin/lumen-appearance"; };
    };
    systemd.user.timers.lumen-appearance = {
      Unit.PartOf = [ "graphical-session.target" ];
      Timer = { OnActiveSec = "2s"; OnUnitActiveSec = "60s"; AccuracySec = "1s"; };
      Install.WantedBy = [ "graphical-session.target" ];
    };
    systemd.user.services.lumen-calendar-sync = lib.mkIf cfg.integrations.calendar.enable {
      Unit = {
        Description = "Sync connected iCloud and Google calendars";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        Type = "oneshot";
        ExecStart = "${personalServices}/bin/lumen-calendar --sync-background";
        TimeoutStartSec = "125s";
        UMask = "0077";
      };
    };
    systemd.user.timers.lumen-calendar-sync = lib.mkIf cfg.integrations.calendar.enable {
      Unit.PartOf = [ "graphical-session.target" ];
      Timer = { OnActiveSec = "2min"; OnUnitInactiveSec = "15min"; RandomizedDelaySec = "30s"; };
      Install.WantedBy = [ "graphical-session.target" ];
    };
    xdg.desktopEntries.lumen-files = {
      name = "Files"; genericName = "File browser";
      exec = "${filesLauncher}/bin/lumen-files %U";
      mimeType = [ "inode/directory" ];
      icon = "${assets}/shell/reference-icons/files.svg";
      terminal = false; categories = [ "System" "FileManager" ];
    };
    xdg.desktopEntries.lumen-system-settings = {
      name = "System Settings";
      genericName = "Desktop preferences";
      exec = "${settingsLauncher}/bin/lumen-system-settings";
      icon = "${assets}/shell/reference-icons/settings.svg";
      terminal = false;
      categories = [ "Settings" ];
    };
    xdg.desktopEntries.lumen-remote-desktop = {
      name = "Remote Desktop";
      exec = "${remoteDesktopLauncher}/bin/lumen-remote-desktop";
      icon = "preferences-desktop-remote-desktop";
      terminal = false;
      categories = [ "Network" "RemoteAccess" ];
    };
    xdg.desktopEntries.lumen-calendar-app = {
      name = "Calendar";
      exec = "${calendarLauncher}/bin/lumen-calendar-app";
      icon = "${assets}/shell/reference-icons/calendar.svg";
      categories = [ "Office" "Calendar" ];
      terminal = false;
    };
    xdg.configFile."foot/foot.ini".source = "${assets}/foot.ini";
    wayland.windowManager.sway.config.keybindings = lib.mkOptionDefault (desktopKeybindings // {
      "Mod4+n" = lib.mkForce desktopKeybindings."Mod4+n";
    });
    xdg.configFile."lumen-desktop/shortcuts.json".source = shortcutRegistry;

    # Use HM's Qt plugin discovery, but let the patched adapter choose its style
    # dynamically. HM's gnome preset requires a static QT_STYLE_OVERRIDE.
    qt.enable = true;
    home.sessionVariables.QT_QPA_PLATFORMTHEME = "gnome";
    systemd.user.sessionVariables.QT_QPA_PLATFORMTHEME = "gnome";
    home.packages = [ readerCommand networkLocations clipboardHistory desktopState remoteDesktopLauncher auth.lock calendarLauncher personalServices qtTheme qtTheme5 desktopDoctor menuCommand desktopAction launchApp notifications audioKey pkgs.trash-cli filesLauncher settingsLauncher pkgs.ddcutil pkgs.foot quickshell shellLauncher appearance nixSettings appLauncher ];
    # Consumers explicitly opt in to replacing competing bar services.
    programs.waybar.systemd.enable = lib.mkIf cfg.integrations.replaceExistingShell (lib.mkForce false);
    wayland.windowManager.sway.config.output."*".bg = lib.mkDefault "${assets}/shell/wallpaper.png fill";
    services.mako.enable = lib.mkIf cfg.integrations.replaceExistingShell (lib.mkForce false);
    xdg.dataFile."dbus-1/services/org.freedesktop.Notifications.service".text = ''
      [D-BUS Service]
      Name=org.freedesktop.Notifications
      Exec=${notifications}/bin/lumen-notifications serve
      SystemdService=lumen-notifications.service
    '';

    # Sway's idle manager issues swaymsg commands, so keep it in the Sway session.
    services.swayidle.systemdTargets = [ "sway-session.target" ];
    systemd.user.services = {
      lumen-hot-corners = {
        Unit = { Description = "Native corner action policy"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" ]; };
        Service = { ExecStart = "${hotCorners}/bin/lumen-hot-corners watch"; Restart = "on-failure"; RestartSec = 2; UMask = "0077"; Environment = [ "XDG_STATE_HOME=${config.xdg.stateHome}" ]; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-spaces = {
        Unit = { Description = "Application desktop assignments and recent desktop order"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" ]; };
        Service = { ExecStart = "${spaces}/bin/lumen-spaces watch"; Restart = "on-failure"; RestartSec = 2; UMask = "0077"; Environment = [ "XDG_STATE_HOME=${config.xdg.stateHome}" ]; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-clipboard-history = lib.mkIf cfg.integrations.clipboard.enable {
        Unit = { Description = "Opt-in ephemeral text clipboard history"; PartOf = [ "graphical-session.target" ]; After = [ "sway-session.target" ]; ConditionPathExists = "${config.xdg.stateHome}/lumen-desktop/clipboard/consent"; };
        Service = { ExecStart = "${clipboardHistory}/bin/lumen-clipboard-history watch"; Restart = "on-failure"; RestartSec = 2; UMask = "0077"; Environment = [ "XDG_STATE_HOME=${config.xdg.stateHome}" ]; StandardOutput = "null"; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-nix-settings = {
        Unit = { Description = "Build authoritative desktop Nix settings"; PartOf = [ "graphical-session.target" ]; Before = [ "lumen-desktop-shell.service" "lumen-display-layout.service" "lumen-appearance.service" ]; };
        Service = { Type = "oneshot"; ExecStart = "${nixSettings}/bin/lumen-nix-settings sync"; RemainAfterExit = true; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-wallpaper-preferences = {
        Unit = { Description = "Apply selected per-display backgrounds"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" "lumen-nix-settings.service" ]; Requires = [ "lumen-nix-settings.service" ]; };
        Service = { ExecStart = "${wallpaperPreferences}/bin/lumen-wallpaper-preferences"; Restart = "on-failure"; RestartSec = 3; UMask = "0077"; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-input-preferences = {
        Unit = { Description = "Apply saved per-device input preferences"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" "lumen-nix-settings.service" ]; Requires = [ "lumen-nix-settings.service" ]; };
        Service = { ExecStart = "${inputPreferences}/bin/lumen-input-preferences"; Restart = "on-failure"; RestartSec = 3; UMask = "0077"; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-display-layout = {
        Unit = { Description = "Apply Nix monitor layouts"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" "lumen-nix-settings.service" ]; Requires = [ "lumen-nix-settings.service" ]; };
        Service = { ExecStart = "${displayLayout}/bin/lumen-display-layout"; Restart = "on-failure"; RestartSec = 3; UMask = "0077"; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-file-operations = {
        Unit = { Description = "Files background file operations"; PartOf = [ "graphical-session.target" ]; };
        Service = { ExecStart = "${fileOperations}/bin/lumen-file-operations"; Restart = "on-failure"; UMask = "0077"; TimeoutStopSec = 120; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-notifications = {
        Unit = { Description = "Desktop notification service and history"; PartOf = [ "graphical-session.target" ]; After = [ "lumen-nix-settings.service" ]; Requires = [ "lumen-nix-settings.service" ]; };
        Service = {
          Type = "dbus";
          BusName = "org.freedesktop.Notifications";
          # Nix wraps Mako as .mako-wrapped; match both executable names.
          ExecStartPre = lib.mkIf cfg.integrations.replaceExistingShell "-${pkgs.procps}/bin/pkill -x (mako|[.]mako-wrapped)";
          ExecStart = "${notifications}/bin/lumen-notifications serve";
          Restart = "on-failure";
          RestartSec = 2;
          UMask = "0077";
        };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-desktop-shell = {
        Unit = {
          Description = "Lumen-style desktop menu bar and Dock";
          StartLimitIntervalSec = 60;
          StartLimitBurst = 3;
          OnFailure = lib.optional cfg.integrations.shellRecovery.enableFallback "lumen-desktop-recovery.service";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" "lumen-notifications.service" "lumen-file-operations.service" "lumen-nix-settings.service" ];
          Wants = [ "lumen-notifications.service" "lumen-file-operations.service" ];
          Requires = [ "lumen-nix-settings.service" ];
        };
        Service = {
          ExecStart = "${shellLauncher}/bin/lumen-desktop-shell";
          Restart = "on-failure";
          RestartSec = 2;
        };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-desktop-recovery = lib.mkIf cfg.integrations.shellRecovery.enableFallback {
        Unit = { Description = "Independent desktop recovery terminal"; PartOf = [ "graphical-session.target" ]; };
        Service = { Type = "exec"; ExecStart = "${shellRecovery}/bin/lumen-session-recovery"; Restart = "no"; UMask = "0077"; };
      };
    };
  };
  };
}
