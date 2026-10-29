{ pkgs, fontPackage ? pkgs.noto-fonts-color-emoji
, fontFile ? "${fontPackage}/share/fonts/noto/NotoColorEmoji.ttf"
, fontFamily ? "Noto Color Emoji" }:
import ./runtime { inherit pkgs fontPackage fontFile fontFamily; }
