{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-browser-workflows-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  env.LUMEN_BROWSER_GTK_SOURCE = pkgs.fetchurl {
    url = "https://raw.githubusercontent.com/chromium/chromium/154.0.8037.98/ui/gtk/select_file_dialog_linux_gtk.cc";
    sha256 = "75c2bf8b98cdf5cfb20f09f888774c12981676ae58d0d65764e871a93d510040";
  };
  env.LUMEN_GTK_CHOOSER_SOURCE = pkgs.fetchurl {
    url = "https://raw.githubusercontent.com/GNOME/gtk/3.24.52/gtk/gtkfilechooserdialog.c";
    sha256 = "20a740448614374b7c68442bd9ef686b2b59be456987e09e0c3e4590c10782f6";
  };
  env.LUMEN_GTK_DIALOG_HEADER = "${pkgs.gtk3.dev}/include/gtk-3.0/gtk/gtkdialog.h";
}
