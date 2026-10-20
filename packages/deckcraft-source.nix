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
  cargoHash = "sha256-5LjT/3EfDPguMPF4/TD6rpIefLk8+sIN0LD6Am3oDTQ=";
  patches = [ ./patches/deckcraft-pdf-output.patch ./patches/deckcraft-atomic-output.patch ./patches/deckcraft-output-formats.patch ];
  postPatch = ''
    cp ${../res/office/atomic_output.rs} crates/engine/src/lumen_atomic_output.rs
  '';
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.alsa-lib ];
  cargoBuildFlags = [ "--package" "deckcraft" "--package" "deckcraft-cli" ];
  cargoTestFlags = [ "--package" "deckcraft-ui-egui" "--lib" "pdf_output" ];
  postCheck = ''
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
