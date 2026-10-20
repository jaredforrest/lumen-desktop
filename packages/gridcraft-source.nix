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
  cargoHash = "sha256-bbKrNxzWaCgeohB9B0kkXAnncbD7ODJ04TbGrRQ/KXE=";
  patches = [ ./patches/gridcraft-native-close.patch ];
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.alsa-lib ];
  cargoBuildFlags = [ "--package" "gridcraft" "--package" "gridcraft-cli" ];
  cargoTestFlags = [ "--package" "gridcraft-ui-egui" "--lib" "native_close" ];
  RUSTFLAGS = "-D warnings";
  strictDeps = true;
  meta = {
    description = "Pinned spreadsheet provider with protected native close requests";
    homepage = "https://github.com/storytold/gridcraft";
    license = with pkgs.lib.licenses; [ mit asl20 ];
    platforms = [ "x86_64-linux" ];
  };
}
