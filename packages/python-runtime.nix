{ pkgs }:
# Keep the compatibility correction inside desktop runtimes and their tests.
pkgs.python3.override {
  packageOverrides = final: previous: {
    dbus-python = previous.dbus-python.overrideAttrs (old: {
      patches = (old.patches or [ ]) ++ [ ./dbus-python-headers-first.patch ];
    });
    pygobject3 = previous.pygobject3.overrideAttrs (old: {
      patches = (old.patches or [ ]) ++ [ ./pygobject-deprecation-registration.patch ./pygobject-python-headers-first.patch ./pygobject-unused-warnlib-target.patch ./pygobject-lazy-legacy-policy.patch ];
      postPatch = (old.postPatch or "") + ''
        ${pkgs.python3}/bin/python3 ${./check-pygobject-test-scope.py} .
      '';
    });
  };
}
