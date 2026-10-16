# Desktop mechanisms and opt-in session integrations. Host policy belongs to consumers.
{ config, pkgs, lib, ... }:
let
  cfg = config.services.lumen-desktop;
  desktopUser = cfg.user;
  settingsPackage = cfg.settingsPackage;
  hostSwayPackage = config.programs.sway.package;
  assets = import ../packages/assets.nix { inherit pkgs; inherit (cfg.appearance) icons wallpaper brandMark; };
  desktopTimeZone = config.time.timeZone;
  auth = import ../packages/lumen-auth.nix { inherit pkgs lib assets; inherit (cfg.integrations.greeter) greeterExtraConfig allowUnsupportedGpu; user = desktopUser; displayName = cfg.displayName; swayPackage = config.programs.sway.package; };
in {
  options.services.lumen-desktop = {
    enable = lib.mkEnableOption "Lumen desktop session";
    user = lib.mkOption { type = lib.types.str; description = "Desktop login user."; };
    displayName = lib.mkOption { type = lib.types.str; default = cfg.user; description = "Name shown by the greeter and lock screen."; };
    settingsPackage = lib.mkOption { type = lib.types.package; description = "Host settings command implementing the settings JSON contract."; };
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
      hyprland.enable = lib.mkEnableOption "Hyprland session integration";
      calendar.enable = lib.mkEnableOption "calendar account services";
      bluetooth.enable = lib.mkEnableOption "Bluetooth UI integration";
      accessibility.enable = lib.mkEnableOption "desktop accessibility bus";
      replaceExistingShell = lib.mkEnableOption "replacement of existing bar and notification services";
    };
  };
  config = lib.mkIf cfg.enable {
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
      emojiFamily = cfg.appearance.emojiFamily;
      user = config.home.username;
      displayName = cfg.displayName;
      homeDirectory = config.home.homeDirectory;
      timeZone = desktopTimeZone;
      swayPackage = hostSwayPackage;
      rofiPackage = config.programs.rofi.package;
    };
    inherit (session) nixSettings appearance qtTheme qtTheme5 palette menuRuntime personalServices fileClipboard displayLayout fileOperations focusWindow launchApp shellLauncher audioKey notifications menuCommand desktopDoctor desktopAction appLauncher filesLauncher remoteDesktopLauncher calendarLauncher settingsLauncher;
    hypr = "${pkgs.hyprland}/bin/hyprctl";
    lock = "${auth.lock}/bin/lumen-lock";
    baseBar = builtins.head config.programs.waybar.settings;
    logout = pkgs.writeShellScript "hyprland-confirm-logout" ''
      choice=$(printf 'Cancel\nLog out\n' | ${config.programs.rofi.package}/bin/rofi -dmenu -p "Session")
      if [ "$choice" = "Log out" ]; then ${hypr} dispatch exit; fi
    '';
    sessionService = description: command: lib.mkIf cfg.integrations.hyprland.enable {
      Unit = {
        Description = description;
        PartOf = [ "hyprland-session.target" ];
        After = [ "hyprland-session.target" ];
      };
      Service = { ExecStart = command; Restart = "on-failure"; };
      Install.WantedBy = [ "hyprland-session.target" ];
    };
  in {
    # Own the theme declaratively, rather than keeping temporary config links.
    wayland.windowManager.sway.extraConfig = lib.mkAfter (builtins.readFile ../res/lumen-desktop/first-pass.conf + ''
      # libinput exposes three-finger swipes; two-finger edge gestures are not available in Sway.
      bindgesture swipe:3:left exec ${desktopAction}/bin/lumen-desktop-action notifications-open
      bindgesture swipe:3:right exec ${desktopAction}/bin/lumen-desktop-action notifications-close
    '');
    programs.waybar.style = lib.mkDefault ("${assets}/bar.css");
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
    wayland.windowManager.sway.config.keybindings = lib.mkOptionDefault ({
      "Mod4+Tab" = "exec ${desktopAction}/bin/lumen-desktop-action overview";
      "Mod4+grave" = "exec ${desktopAction}/bin/lumen-desktop-action app-windows";
      "Mod4+Mod1+t" = "exec ${desktopAction}/bin/lumen-desktop-action text-tools";
      "Mod4+n" = lib.mkForce "exec ${desktopAction}/bin/lumen-desktop-action notifications-toggle";
      "Control+F3" = "exec ${desktopAction}/bin/lumen-desktop-action dock";
      "XF86AudioRaiseVolume" = "exec ${audioKey}/bin/lumen-audio-key up";
      "XF86AudioLowerVolume" = "exec ${audioKey}/bin/lumen-audio-key down";
      "XF86AudioMute" = "exec ${audioKey}/bin/lumen-audio-key mute";
      "XF86AudioPlay" = "exec ${pkgs.playerctl}/bin/playerctl play-pause";
      "XF86AudioNext" = "exec ${pkgs.playerctl}/bin/playerctl next";
      "XF86AudioPrev" = "exec ${pkgs.playerctl}/bin/playerctl previous";
    } // builtins.listToAttrs (map (n: { name = "Mod4+Mod1+F${toString n}"; value = "exec ${menuCommand}/bin/lumen-menu-command slot ${toString n}"; }) (lib.range 1 12)));
    # Use HM's Qt plugin discovery, but let the patched adapter choose its style
    # dynamically. HM's gnome preset requires a static QT_STYLE_OVERRIDE.
    qt.enable = true;
    home.sessionVariables.QT_QPA_PLATFORMTHEME = "gnome";
    systemd.user.sessionVariables.QT_QPA_PLATFORMTHEME = "gnome";
    home.packages = [ remoteDesktopLauncher auth.lock calendarLauncher personalServices qtTheme qtTheme5 desktopDoctor menuCommand desktopAction launchApp notifications audioKey pkgs.trash-cli filesLauncher settingsLauncher pkgs.ddcutil pkgs.foot pkgs.quickshell shellLauncher appearance nixSettings appLauncher ];
    # Keep Waybar available as a manual fallback, but do not run two shell bars.
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
      lumen-nix-settings = {
        Unit = { Description = "Build authoritative desktop Nix settings"; PartOf = [ "graphical-session.target" ]; Before = [ "lumen-desktop-shell.service" "lumen-display-layout.service" "lumen-appearance.service" ]; };
        Service = { Type = "oneshot"; ExecStart = "${nixSettings}/bin/lumen-nix-settings sync"; RemainAfterExit = true; };
        Install.WantedBy = [ "sway-session.target" "hyprland-session.target" ];
      };
      lumen-display-layout = {
        Unit = { Description = "Apply Nix monitor layouts"; PartOf = [ "sway-session.target" ]; After = [ "sway-session.target" "lumen-nix-settings.service" ]; Requires = [ "lumen-nix-settings.service" ]; };
        Service = { ExecStart = "${displayLayout}/bin/lumen-display-layout"; Restart = "on-failure"; RestartSec = 3; UMask = "0077"; };
        Install.WantedBy = [ "sway-session.target" ];
      };
      lumen-file-operations = {
        Unit = { Description = "Files background file operations"; PartOf = [ "graphical-session.target" ]; };
        Service = { ExecStart = "${fileOperations}/bin/lumen-file-operations"; Restart = "on-failure"; UMask = "0077"; TimeoutStopSec = 120; };
        Install.WantedBy = [ "sway-session.target" "hyprland-session.target" ];
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
        Install.WantedBy = [ "sway-session.target" "hyprland-session.target" ];
      };
      lumen-desktop-shell = {
        Unit = {
          Description = "Lumen-style desktop menu bar and Dock";
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
        Install.WantedBy = [ "sway-session.target" "hyprland-session.target" ];
      };
      hyprland-wallpaper = sessionService "Hyprland wallpaper"
        "${pkgs.swaybg}/bin/swaybg -i ${assets}/shell/wallpaper.png -m fill";
      hyprland-polkit = sessionService "Hyprland authentication agent"
        "${pkgs.polkit_gnome}/libexec/polkit-gnome-authentication-agent-1";
      hyprland-clipboard = sessionService "Hyprland persistent clipboard"
        "${pkgs.wl-clip-persist}/bin/wl-clip-persist --clipboard regular";

    };

    wayland.windowManager.hyprland = lib.mkIf cfg.integrations.hyprland.enable {
      configType = "hyprlang";
      enable = true;
      # NixOS supplies the compositor and matching screen-sharing portal.
      package = null;
      portalPackage = null;
      systemd.enable = true;
      settings = {
        exec-shutdown = "${pkgs.systemd}/bin/systemctl --user stop hyprland-session.target";
        general = {
          gaps_in = 4;
          gaps_out = 12;
          border_size = 1;
          "col.active_border" = "rgba(7aa2d6cc)";
          "col.inactive_border" = "rgba(42444b99)";
          layout = "dwindle";
          resize_on_border = true;
        };
        decoration = {
          rounding = 12;
          active_opacity = 1.0;
          inactive_opacity = 1.0;
          shadow = { enabled = true; range = 18; render_power = 3; color = "rgba(00000035)"; };
          blur = { enabled = true; size = 4; passes = 2; vibrancy = 0.12; };
        };
        animations = {
          enabled = true;
          bezier = [ "gentle,0.22,1,0.36,1" ];
          animation = [
            "windows,1,3,gentle,slide"
            "windowsIn,1,3,gentle,popin 96%"
            "windowsOut,1,2,gentle,popin 98%"
            "fade,1,2,gentle"
            "workspaces,1,3.5,gentle,slide"
            "border,1,2,gentle"
          ];
        };
        dwindle = { pseudotile = true; preserve_split = true; };
        misc = { disable_hyprland_logo = true; disable_splash_rendering = true; force_default_wallpaper = 0; };
        layerrule = [ "blur,lumen-menu" "ignorezero,lumen-menu" "blur,lumen-dock" "ignorezero,lumen-dock" ];
        bind = [
          "SUPER, Return, exec, ${pkgs.foot}/bin/foot"
          "SUPER, P, exec, ${appLauncher}/bin/lumen-app-launcher"
          "SUPER SHIFT, Q, killactive,"
          "SUPER SHIFT, E, exec, ${logout}"
          "SUPER CTRL, L, exec, ${lock}"
          "SUPER CTRL, Q, exec, ${lock}"
          "SUPER, F, fullscreen, 0"
          "SUPER SHIFT, SPACE, togglefloating,"
          "SUPER, V, togglesplit,"
          "SUPER, M, exec, ${audioKey}/bin/lumen-audio-key mute"
          "SUPER SHIFT, M, exec, ${pkgs.wireplumber}/bin/wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"
          "SUPER, N, exec, ${notifications}/bin/lumen-notifications dnd"
          "SUPER, C, exec, ${baseBar."custom/clock".on-click}"
          "SUPER SHIFT, B, exec, ${pkgs.rofi-bluetooth}/bin/rofi-bluetooth"
          "SUPER, minus, togglespecialworkspace, scratchpad"
          "SUPER SHIFT, minus, movetoworkspace, special:scratchpad"
        ] ++ lib.concatLists (lib.imap0 (i: key:
          let direction = builtins.elemAt [ "l" "d" "u" "r" ] i;
              arrow = builtins.elemAt [ "left" "down" "up" "right" ] i;
          in [
            "SUPER, ${key}, movefocus, ${direction}"
            "SUPER, ${arrow}, movefocus, ${direction}"
            "SUPER SHIFT, ${key}, movewindow, ${direction}"
            "SUPER SHIFT, ${arrow}, movewindow, ${direction}"
          ]) [ "H" "J" "K" "L" ])
          ++ lib.concatMap (i: let key = if i == 10 then "0" else toString i; in [
            "SUPER, ${key}, workspace, ${toString i}"
            "SUPER SHIFT, ${key}, movetoworkspace, ${toString i}"
          ]) (lib.range 1 10);
        bindm = [ "SUPER, mouse:272, movewindow" "SUPER, mouse:273, resizewindow" ];
        bindel = [
          ", XF86AudioRaiseVolume, exec, ${audioKey}/bin/lumen-audio-key up"
          ", XF86AudioLowerVolume, exec, ${audioKey}/bin/lumen-audio-key down"
        ];
        bindl = [
          ", XF86AudioMute, exec, ${audioKey}/bin/lumen-audio-key mute"
          ", XF86AudioPlay, exec, ${pkgs.playerctl}/bin/playerctl play-pause"
        ];
      };
    };
  };
  };
}
