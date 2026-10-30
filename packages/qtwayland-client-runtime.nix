{ pkgs }:
let qt = pkgs.qt6.qtbase;
in pkgs.stdenv.mkDerivation {
  dontWrapQtApps = true;
  pname = "lumen-qt-wayland-client-runtime";
  inherit (qt) version src;
  patches = (qt.patches or [ ]) ++ [ ./qtwayland-destroyed-focus-release.patch ./qtwayland-empty-preedit-composition.patch ];
  nativeBuildInputs = (qt.nativeBuildInputs or [ ]) ++ [ pkgs.cmake pkgs.ninja pkgs.pkg-config pkgs.wayland-scanner pkgs.patchelf pkgs.binutils qt ];
  buildInputs = (qt.buildInputs or [ ]) ++ (qt.propagatedBuildInputs or [ ]) ++ [ qt pkgs.wayland pkgs.wayland-protocols pkgs.libxkbcommon ];
  NIX_CFLAGS_COMPILE = qt.NIX_CFLAGS_COMPILE or "";
  postPatch = ''
    waylandCmake=src/plugins/platforms/wayland/CMakeLists.txt
    test "$(grep -c '^qt_internal_add_plugin(QWaylandIntegrationPlugin$' "$waylandCmake")" -eq 1
    test "$(grep -c '^qt6_generate_wayland_protocol_client_sources' "$waylandCmake")" -ge 1
    test "$(grep -c '^    add_subdirectory(plugins)$' "$waylandCmake")" -eq 1
    sed -i '/^qt_internal_add_plugin(QWaylandIntegrationPlugin$/,/^qt6_generate_wayland_protocol_client_sources/{ /^qt6_generate_wayland_protocol_client_sources/!d; }' "$waylandCmake"
    sed -i '/^    add_subdirectory(plugins)$/d' "$waylandCmake"
    cat > CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.22)
include(.cmake.conf)
project(QtWaylandClientRuntime VERSION 6.11.2 LANGUAGES C CXX)
find_package(Qt6 6.11.2 EXACT CONFIG REQUIRED COMPONENTS BuildInternals Core CorePrivate Gui GuiPrivate Network DBus OpenGL)
find_package(Wayland REQUIRED COMPONENTS Client Cursor)
find_package(WaylandScanner REQUIRED)
find_package(Qt6WaylandScannerTools 6.11.2 EXACT CONFIG REQUIRED)
qt_internal_project_setup()
qt_build_repo_begin()
add_subdirectory(src/plugins/platforms/wayland)
qt_build_repo_post_process()
qt_build_repo_end()
EOF
  '';
  configurePhase = ''
    runHook preConfigure
    set -o pipefail
    cmake -S . -B build -G Ninja -DCMAKE_INSTALL_PREFIX="$out" -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS=-Werror 2>&1 | tee configure.log
    if grep -E 'CMake Warning|[Ww]arning:' configure.log; then
      echo "Qt Wayland client configuration emitted a warning" >&2
      exit 1
    fi
    runHook postConfigure
  '';
  buildPhase = ''
    runHook preBuild
    set -o pipefail
    cmake --build build --parallel "$NIX_BUILD_CORES" --target WaylandClient 2>&1 | tee build.log
    if grep -E 'CMake Warning|[Ww]arning:' build.log; then
      echo "Qt Wayland client build emitted a warning" >&2
      exit 1
    fi
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/lib"
    cp -a build/lib/libQt6WaylandClient.so* "$out/lib/"
    test "$(patchelf --print-soname "$out/lib/libQt6WaylandClient.so")" = "$(patchelf --print-soname ${qt}/lib/libQt6WaylandClient.so)"
    nm -D --defined-only ${qt}/lib/libQt6WaylandClient.so | awk '{print $3}' | sort > pinned-symbols
    nm -D --defined-only "$out/lib/libQt6WaylandClient.so" | awk '{print $3}' | sort > rebuilt-symbols
    cmp pinned-symbols rebuilt-symbols
    mkdir -p "$out/share/lumen-desktop/abi"
    for configHeader in qtwaylandclient-config.h qtwaylandclient-config_p.h; do
      builtHeader=$(find build/src/plugins/platforms/wayland -type f -name "$configHeader")
      pinnedHeader=$(find ${qt}/include/QtWaylandClient -type f -name "$configHeader")
      test -n "$builtHeader" && test -n "$pinnedHeader"
      cmp "$builtHeader" "$pinnedHeader"
      cp "$builtHeader" "$out/share/lumen-desktop/abi/$configHeader"
    done
    runHook postInstall
  '';
  meta.license = qt.meta.license;
}
