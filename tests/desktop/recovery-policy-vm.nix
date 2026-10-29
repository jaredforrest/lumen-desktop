{ nixpkgs, system }:
let
  pkgs = import nixpkgs { inherit system; };
  fixture = import ./session-recovery-rust { inherit pkgs; };
  recovery = import ../../packages/session-recovery.nix {
    inherit pkgs;
    terminal = "${fixture}/bin/foot";
    shell = "${fixture}/bin/recovery-shell";
  };
  consumer = nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [ ../../modules/desktop.nix ({ lib, ... }: {
      options.home-manager.users = lib.mkOption { type = lib.types.attrsOf lib.types.raw; default = {}; };
      config = {
        system.stateVersion = "26.05";
        services.lumen-desktop = { enable = true; user = "fixture"; settingsPackage = pkgs.hello;
          integrations.shellRecovery.fallbackPackage = recovery; };
      };
    }) ];
  };
  home = consumer.config.home-manager.users.fixture { config = {}; lib = nixpkgs.lib; };
  shell = home.systemd.user.services.lumen-desktop-shell;
  fallback = (nixpkgs.lib.evalModules { modules = [ ({ lib, ... }: {
    options.value = lib.mkOption { type = lib.types.attrs; };
    config.value = home.systemd.user.services.lumen-desktop-recovery;
  }) ]; }).config.value;
  disabled = (consumer.extendModules { modules = [ {services.lumen-desktop.integrations.shellRecovery.enableFallback = false;} ]; }).config;
  disabledHome = disabled.home-manager.users.fixture { config = {}; lib = nixpkgs.lib; };
  disabledShell = disabledHome.systemd.user.services.lumen-desktop-shell;
  driver = pkgs.runCommand "lumen-recovery-policy-driver" {
    nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
    VM_RUNUSER = "${pkgs.util-linux}/bin/runuser";
    VM_ENV = "${pkgs.coreutils}/bin/env";
    VM_SYSTEMCTL = "${pkgs.systemd}/bin/systemctl";
  } ''
    mkdir -p "$out/bin"
    rustc --edition=2024 -D warnings ${./recovery-policy-vm.rs} -o "$out/bin/lumen-recovery-policy"
  '';
  unit = original: counter: {
    unitConfig = builtins.removeAttrs original.Unit [ "Description" "After" "Wants" "Requires" ];
    serviceConfig = original.Service // { ExecStart = "${driver}/bin/lumen-recovery-policy crash /home/fixture/${counter}"; };
  };
  vm = nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [ (nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix") ({ lib, ... }: {
      system.stateVersion = "26.05";
      nix.enable = false;
      networking.hostName = "recovery-policy";
      networking.useDHCP = false;
      users.users.fixture = { isNormalUser = true; uid = 1000; linger = true; hashedPassword = "!"; };
      virtualisation = {
        graphics = false; memorySize = 1024; cores = 2; diskSize = 1024;
        useNixStoreImage = true; mountHostNixStore = false; writableStore = false;
        sharedDirectories = lib.mkForce {};
        qemu.forceAccel = false;
        qemu.options = [ "-machine accel=tcg" ];
        qemu.networkingOptions = lib.mkForce [ "-nic none" ];
      };
      systemd.user.targets.recovery-test-session = {
        description = "Disposable graphical session owner";
        requires = [ "graphical-session.target" ];
        after = [ "graphical-session.target" ];
        unitConfig.PartOf = [ "graphical-session.target" ];
      };
      systemd.user.services.lumen-desktop-shell = unit shell "attempts";
      systemd.user.services.recovery-disabled = unit disabledShell "disabled-attempts";
      systemd.user.services.lumen-desktop-recovery = {
        unitConfig = builtins.removeAttrs fallback.Unit [ "Description" ];
        serviceConfig = fallback.Service // {
          Environment = [ "RECOVERY_TERMINAL=/home/fixture/fallback-terminal" "RECOVERY_SHELL_RECEIPT=/home/fixture/fallback-shell" "RECOVERY_SHELL_HOLD=1" ];
        };
      };
      systemd.user.services.recovery-client = {
        unitConfig.PartOf = [ "graphical-session.target" ];
        serviceConfig.ExecStart = "${driver}/bin/lumen-recovery-policy client";
      };
      systemd.services.recovery-policy-acceptance = {
        wantedBy = [ "multi-user.target" ]; after = [ "user@1000.service" ]; wants = [ "user@1000.service" ];
        serviceConfig = { Type = "oneshot"; ExecStart = "${driver}/bin/lumen-recovery-policy driver"; StandardOutput = "journal+console"; StandardError = "journal+console"; };
      };
    }) ];
  };
in pkgs.runCommand "lumen-recovery-policy-vm" { nativeBuildInputs = [ pkgs.coreutils ]; } ''
  mkdir -p "$out"
  export NIX_DISK_IMAGE="$TMPDIR/recovery-policy.qcow2"
  timeout 300 ${vm.config.system.build.vm}/bin/run-recovery-policy-vm > "$out/serial.log" 2>&1
  grep -q LUMEN_RECOVERY_POLICY_PASS "$out/serial.log"
  if grep -q LUMEN_RECOVERY_POLICY_FAIL "$out/serial.log"; then exit 1; fi
''
