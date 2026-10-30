{ pkgs }:
let
  fonts = pkgs.fetchFromGitHub {
    owner = "storytold";
    repo = "craft-fonts";
    rev = "43913056ce82a3fdad0ef69f6b88d1dc922a8104";
    hash = "sha256-u1fC5spekizPXnrK6uiKEZSr2SJOaBF4ffLDbKkywEY=";
  };
in pkgs.rustPlatform.buildRustPackage {
  pname = "deckcraft-native";
  version = "0.5.0";
  src = pkgs.fetchFromGitHub {
    owner = "storytold";
    repo = "deckcraft";
    rev = "a98df49b21632d69a3da994d35df1ea91dab41af";
    hash = "sha256-B+kuyp/5j/yKqaQgood7VL4Vhf0gnOSTqT43VYa1LwE=";
  };
  cargoHash = "sha256-RqOeQpB7m4/9X74YC3gdrwUQfBGctVUIRIkREjzMEoI=";
  cargoPatches = [ ./patches/deckcraft-history-lock.patch ];
  patches = [ ./patches/deckcraft-pdf-output.patch ./patches/deckcraft-atomic-output.patch ./patches/deckcraft-output-formats.patch ./patches/deckcraft-durable-history.patch ];
  postPatch = ''
    cp ${../res/office/atomic_output.rs} crates/engine/src/lumen_atomic_output.rs
    cp ${../res/office/atomic_output.rs} crates/atomic_output.rs
    cp ${../res/office/deck_history.rs} crates/engine/src/lumen_history.rs
    cp ${../res/office/deck_history_ui.rs} crates/ui-egui/src/history_ui.rs
    cp -r ${../res/office/document-history} crates/lumen-document-history
  '';
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.alsa-lib ];
  cargoBuildFlags = [ "--package" "deckcraft" "--package" "deckcraft-cli" ];
  cargoTestFlags = [ "--package" "deckcraft-ui-egui" "--lib" "pdf_output" ];
  postCheck = ''
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package deckcraft-engine --lib persistent_history -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package deckcraft-ui-egui --lib persistent_history -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package deckcraft-engine --lib lumen_atomic_output -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package deckcraft-engine --lib output_formats -j "$NIX_BUILD_CORES"
  '';
  CRAFT_FONTS_DIR = fonts;
  CRAFT_FONTS_REQUIRED = "1";
  RUSTFLAGS = "-D warnings";
  strictDeps = true;
  meta = {
    description = "Pinned presentation provider with captured PDF output";
    license = with pkgs.lib.licenses; [ mit asl20 ];
    platforms = [ "x86_64-linux" ];
  };
}
