function timePattern(locale,prefs) {
    let pattern=locale.timeFormat(Locale.ShortFormat);
    if(prefs.hourCycle==="12")pattern="h:mm AP";
    else if(prefs.hourCycle==="24")pattern="HH:mm";
    if(prefs.showSeconds)pattern=pattern.replace(/m+/,match=>match+":ss");
    return pattern;
}
function datePattern(locale,prefs) {
    const order=locale.dateFormat(Locale.LongFormat).replace(/d{3,4}/g, "").replace(/'[^']*'/g, "");
    const day=order.indexOf("M")<order.indexOf("d")?"MMM d":"d MMM";
    return (prefs.showWeekday?"ddd":"")+(prefs.showDate?(prefs.showWeekday?" ":"")+day:"");
}
function text(date,locale,prefs) { const pattern=datePattern(locale,prefs);return (pattern?date.toLocaleString(locale,pattern)+"  ":"")+date.toLocaleTimeString(locale,timePattern(locale,prefs)); }
