# Dock motion and reorder acceptance

The Rust 2024 `dock-motion` desktop-policy scenario sends continuous pointer movement across the production Dock at three step rates, observes every neighbour tooltip and settled magnification, then leaves the shelf and checks that tooltip, hover and scale all clear. It performs successful forward and reverse app reorder beside the recent/document separators and refuses a document-section destination without activating or removing anything. CI runs it on the basic and effects compositors.

The scenario also covers releasing a non-draggable item over blank shelf space,
recovering neighbour tooltips, dismissal by another popup during a held drag,
keyboard selection magnification, Escape, and removal of the selected model.
These cases must leave no activation or preference writes. An empty model must
not claim exclusive keyboard focus. Pointer motion returns keyboard ownership
to the application, and a release resolves its actual coordinates rather than a
previous hover position.

The production Dock, SettingsState and DockModel are unchanged. Read-only geometry/tooltip aliases are applied to a copied source file with original/modified hashes and exact round-trip proof. An explicit synthetic settings transport records the actual captured pin-order write and reconstructs that order after a private shell restart. This establishes the owned UI/transport contract; it does not claim production configuration or system rebuild persistence. Existing production appearance preference tests cover the separate typed schema boundary; production configuration writes remain outside this fixture.

Every run uses a private headless compositor, session bus, disposable home, copied production assets and pinned Nix tools. The settings transport is a Rust fixture binary. No reference-platform timing or pixel equivalence is inferred from this functional acceptance.
