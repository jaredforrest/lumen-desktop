function iso(d) { return d.getFullYear()+"-"+String(d.getMonth()+1).padStart(2,"0")+"-"+String(d.getDate()).padStart(2,"0"); }
function monthShift(d, delta) { const last=new Date(d.getFullYear(),d.getMonth()+delta+1,0).getDate(); return new Date(d.getFullYear(),d.getMonth()+delta,Math.min(d.getDate(),last)); }
function offset(month, firstDay) { return (month.getDay()-(firstDay%7)+7)%7; }
function tint(event, dark) {
    // Keep valid provider colours; generate stable identities for older caches.
    if (/^#[0-9a-fA-F]{6}$/.test(event.color || "")) return event.color;
    const colors=dark?["#bf7af8","#ffb340","#70b7ff","#65ce94","#ff7ba0"]:["#9253c7","#a86100","#1265b0","#23834b","#b82d58"];
    let hash=0;for(const c of (event.provider || "")+" / "+(event.calendar || ""))hash=(hash*31+c.charCodeAt(0))>>>0;
    return colors[hash%colors.length];
}

function weekdayNames(locale, firstDay, format) { return Array.from({length:7},(_,i)=>locale.standaloneDayName((firstDay+i)%7,format)); }

function clockDatePattern(locale, longFormat) {
    const order=locale.dateFormat(longFormat).replace(/d{3,4}/g, "").replace(/'[^']*'/g, "");
    return order.indexOf("M")<order.indexOf("d")?"ddd MMM d":"ddd d MMM";
}
