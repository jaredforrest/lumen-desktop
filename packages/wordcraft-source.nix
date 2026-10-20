{ pkgs }:
let
  fonts = pkgs.fetchFromGitHub {
    owner = "storytold";
    repo = "craft-fonts";
    rev = "8dcdacd5153e64560d109541a47d806f26f048c0";
    hash = "sha256-4E9OrthE5z39xFXpvpwn4Fn6RH3wX2sF56rZZejZT+I=";
  };
in pkgs.rustPlatform.buildRustPackage {
  pname = "wordcraft-native";
  version = "0.5.0";
  src = pkgs.fetchFromGitHub {
    owner = "storytold";
    repo = "wordcraft";
    rev = "e4b7d155f15a7784197ba34eeea0b54490d70eb2";
    hash = "sha256-yJk5hzvvjwJXJkZFc/dO+Llf70w7xZnoX7zI3ifIuoY=";
  };
  cargoHash = "sha256-OljEFWaPOT3nmWU/xVTcbn3ePAdEYkhG6/U4ysVG9Kc=";
  patches = [ ./patches/wordcraft-print-failure.patch ./patches/wordcraft-version-ownership.patch ./patches/wordcraft-atomic-output.patch ./patches/wordcraft-chooser-ownership.patch ];
  postPatch = ''
    cp ${../res/office/atomic_output.rs} crates/engine/src/lumen_atomic_output.rs
  '';
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.alsa-lib ];
  cargoBuildFlags = [ "--package" "wordcraft" "--package" "wordcraft-cli" ];
  cargoTestFlags = [ "--package" "wordcraft-ui-egui" "--lib" "print" ];
  postCheck = ''
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package wordcraft-ui-egui --lib file_dialogs -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package wordcraft-engine --lib lumen_atomic_output -j "$NIX_BUILD_CORES"
    cargo test --offline --profile release --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget} --package wordcraft-engine --lib document_versions_ownership -j "$NIX_BUILD_CORES"
  '';
  CRAFT_FONTS_DIR = fonts;
  CRAFT_FONTS_REQUIRED = "1";
  RUSTFLAGS = "-D warnings";
  strictDeps = true;
  meta = {
    description = "Pinned document provider with visible print handoff failure";
    license = with pkgs.lib.licenses; [ mit asl20 ];
    platforms = [ "x86_64-linux" ];
  };
}
