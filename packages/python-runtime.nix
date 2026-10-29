{ pkgs }:
# Keep the compatibility correction inside desktop runtimes and their tests.
pkgs.python3.override {
  packageOverrides = final: previous: {
    dbus-python = previous.dbus-python.overrideAttrs (old: {
      patches = (old.patches or [ ]) ++ [ ./dbus-python-headers-first.patch ];
    });
    pygobject3 = previous.pygobject3.overrideAttrs (old: {
      patches = (old.patches or [ ]) ++ [ ./pygobject-deprecation-registration.patch ./pygobject-python-headers-first.patch ./pygobject-unused-warnlib-target.patch ./pygobject-lazy-legacy-policy.patch ];
      nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.nushell ];
      postPatch = (old.postPatch or "") + ''
        ${pkgs.nushell}/bin/nu ${./check-pygobject-test-scope.nu} .
      '';
    });
  };
}
