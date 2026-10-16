// Desktop-only presentation. Transfer, discovery, TLS and consent code is unchanged.
ThemeData desktopAppearance(ThemeData base, Brightness brightness) {
  final dark = brightness == Brightness.dark;
  final background = Color(dark ? 0xff252527 : 0xfff2f2f4);
  final surface = Color(dark ? 0xff333336 : 0xffffffff);
  final field = Color(dark ? 0xff404044 : 0xffe5e5e8);
  final sidebar = Color(dark ? 0xff242426 : 0xffe8e8ec);
  final text = Color(dark ? 0xfff5f5f7 : 0xff202023);
  final muted = Color(dark ? 0xffb0b0b8 : 0xff6b6b70);
  final outline = Color(dark ? 0xff505054 : 0xffccccd0);
  final accent = Color(dark ? 0xff0a84ff : 0xff007aff);
  final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(7));
  return base.copyWith(
    platform: TargetPlatform.linux,
    visualDensity: VisualDensity.compact,
    scaffoldBackgroundColor: background,
    canvasColor: background,
    cardColor: surface,
    dividerColor: outline,
    splashFactory: NoSplash.splashFactory,
    colorScheme: base.colorScheme.copyWith(primary: accent, onPrimary: Colors.white,
      secondary: accent, surface: surface, onSurface: text, surfaceTint: Colors.transparent,
      secondaryContainer: field, onSecondaryContainer: text, outline: outline),
    textTheme: base.textTheme.apply(fontFamily: 'Interlude Variable', bodyColor: text, displayColor: text),
    appBarTheme: AppBarTheme(backgroundColor: background, foregroundColor: text,
      elevation: 0, scrolledUnderElevation: 0, surfaceTintColor: Colors.transparent,
      titleTextStyle: TextStyle(fontFamily: 'Interlude Variable', color: text, fontSize: 17, fontWeight: FontWeight.w600)),
    cardTheme: CardThemeData(color: surface, elevation: 0, surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12))),
    dialogTheme: DialogThemeData(backgroundColor: background, surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12))),
    elevatedButtonTheme: ElevatedButtonThemeData(style: ElevatedButton.styleFrom(
      backgroundColor: surface, foregroundColor: text, elevation: 0, shape: shape,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8))),
    filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(
      backgroundColor: accent, foregroundColor: Colors.white, shape: shape,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8))),
    outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(
      foregroundColor: text, side: BorderSide(color: outline), shape: shape,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8))),
    textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(
      foregroundColor: accent, shape: shape, padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8))),
    inputDecorationTheme: InputDecorationThemeData(filled: true, fillColor: field,
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(7), borderSide: BorderSide(color: outline)),
      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(7), borderSide: BorderSide(color: outline)),
      focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(7), borderSide: BorderSide(color: accent, width: 2)),
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10)),
    popupMenuTheme: PopupMenuThemeData(color: background, surfaceTintColor: Colors.transparent,
      textStyle: TextStyle(fontFamily: 'Interlude Variable', color: text, fontSize: 13),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10))),
    navigationRailTheme: NavigationRailThemeData(backgroundColor: sidebar,
      selectedLabelTextStyle: TextStyle(fontFamily: 'Interlude Variable', color: text, fontSize: 13),
      unselectedLabelTextStyle: TextStyle(fontFamily: 'Interlude Variable', color: muted, fontSize: 13)),
    sliderTheme: base.sliderTheme.copyWith(trackHeight: 4, activeTrackColor: accent,
      inactiveTrackColor: field, thumbColor: Colors.white,
      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 7)),
    dividerTheme: DividerThemeData(color: outline, thickness: 0.5),
  );
}
