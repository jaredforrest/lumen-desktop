{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "gridcraft-native";
  version = "0.5.0";
  src = pkgs.fetchFromGitHub {
    owner = "storytold";
    repo = "gridcraft";
    rev = "9f896c1249f40d18bf182199dc03042959be5f3f";
    hash = "sha256-TFziJLqG7Jmf5DD0xsS4H3WzX7FXeyyezj0kSA7TFw0=";
  };
  cargoHash = "sha256-UxRu5rY+cH/uxbT+c7j2r+t4jbV7V6rd/ZgVGML87Yc=";
  cargoPatches = [ ./patches/gridcraft-history-lock.patch ];
  patches = [ ./patches/gridcraft-native-close.patch ./patches/gridcraft-pdf-output.patch ./patches/gridcraft-atomic-output.patch ./patches/gridcraft-durable-history.patch ];
  postPatch = ''
    cp ${../res/office/atomic_output.rs} crates/engine/src/lumen_atomic_output.rs
    cp ${../res/office/atomic_output.rs} crates/atomic_output.rs
    cp ${../res/office/grid_history.rs} crates/engine/src/lumen_history.rs
    cp ${../res/office/grid_history_ui.rs} crates/ui-egui/src/history_ui.rs
    cp -r ${../res/office/document-history} crates/lumen-document-history
  '';
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.alsa-lib ];
  cargoBuildFlags = [ "--package" "gridcraft" "--package" "gridcraft-cli" ];
  cargoTestFlags = [ "--package" "gridcraft-ui-egui" "--lib" "native_close" ];
  postCheck = ''
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package gridcraft-engine --lib persistent_history -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package gridcraft-ui-egui --lib persistent_history -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package gridcraft-engine --lib lumen_atomic_output -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package gridcraft-ui-egui --lib pdf_output -j "$NIX_BUILD_CORES"
  '';
  RUSTFLAGS = "-D warnings";
  strictDeps = true;
  meta = {
    description = "Pinned spreadsheet provider with protected native close requests";
    homepage = "https://github.com/storytold/gridcraft";
    license = with pkgs.lib.licenses; [ mit asl20 ];
    platforms = [ "x86_64-linux" ];
  };
}
