# Live calendar icon

Calendar labels use the process default locale explicitly. The date property remains live: midnight, month, year and leap-day transitions update both weekday and day. This does not change the host clock or locale.

A failed calendar image uses a neutral calendar backplate in the selected appearance while keeping the live labels. It never puts calendar labels on the generic application image. Other unavailable application images keep their existing generic fallback. An explicit source or appearance change can reload recovered assets; automatic filesystem watching is not provided.

The full-namespace software Qt corpus captures 36 actual date/appearance/locale frames plus four normal/dark recovery frames. It sets a synthetic UTC date input and private English/French process locales, removes only the private calendar asset copies, and restores them through a fixed private filesystem helper. It checks actual labels and frames from production AppIcon; this is component rendering acceptance, not physical Dock clock input, reference typography or arbitrary locale coverage.
