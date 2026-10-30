{ pkgs }:
# Scoped runtime fixes; the system Qt and compiler packages remain unchanged.
let
  waylandRuntime = import ./qtwayland-client-runtime.nix { inherit pkgs; };
in pkgs.quickshell.overrideAttrs (old: {
  nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.patchelf ];
  buildInputs = (old.buildInputs or [ ]) ++ [ pkgs.libsysprof-capture ];
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    identityTestLog="$TMPDIR/lumen-toplevel-identity-test.log"
    ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 UBSAN_OPTIONS=halt_on_error=1 \
      timeout 30s ./src/wayland/toplevel/lumen-toplevel-identity-test -v1 | tee "$identityTestLog"
    runHook postCheck
  '';
  postInstall = (old.postInstall or "") + ''
    patchelf --print-needed "$out/bin/quickshell" | grep -qx libQt6WaylandClient.so.6
    shellRuntimeRpath=$(patchelf --print-rpath "$out/bin/quickshell")
    patchelf --set-rpath "${waylandRuntime}/lib:$shellRuntimeRpath" "$out/bin/quickshell"
    mkdir -p "$out/share/lumen-desktop"
    cp "$identityTestLog" "$out/share/lumen-desktop/toplevel-identity-lifecycle-test.log"
    printf '%s\n' '${waylandRuntime}' > "$out/share/lumen-desktop/qt-wayland-runtime"
  '';
  # These protocol modules are unused; the session talks to its compositor broker.
  cmakeFlags = (old.cmakeFlags or [ ]) ++ [
    "-DHYPRLAND=OFF"
    "-DI3=OFF"
    "-DSCREENCOPY_HYPRLAND_TOPLEVEL=OFF"
    "-DLUMEN_TOPLEVEL_IDENTITY_TESTS=ON"
  ];
  patches = (old.patches or [ ]) ++ [ ./quickshell-accessibility-lifecycle.patch ./quickshell-compiler-corrections.patch ./quickshell-cmake-registry-policy.patch ./quickshell-toplevel-identity.patch ];
})
