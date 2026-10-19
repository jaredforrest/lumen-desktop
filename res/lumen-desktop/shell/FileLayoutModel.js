function sidebarMaximum(windowWidth){return Math.max(120,Math.min(360,Math.floor(windowWidth)-300));}
function sidebarWidth(requested,windowWidth){return Math.max(120,Math.min(sidebarMaximum(windowWidth),Number.isInteger(requested)?requested:190));}
if(typeof module!=="undefined")module.exports={sidebarMaximum,sidebarWidth};
