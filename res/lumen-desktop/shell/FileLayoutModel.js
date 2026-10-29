function sidebarMaximum(windowWidth){return Math.max(120,Math.min(360,Math.floor(windowWidth)-300));}
function sidebarWidth(requested,windowWidth){return Math.max(120,Math.min(sidebarMaximum(windowWidth),Number.isInteger(requested)?requested:190));}
function previewLimits(available,scale){
    const factor=Math.max(1,Math.min(1.5,Number.isFinite(scale)?scale:1));
    const minimum=Math.ceil(160*factor),leadingMinimum=Math.ceil(200*factor);
    const maximum=Math.min(600,Math.floor(available)-leadingMinimum-8);
    return {minimum:minimum,maximum:Math.max(minimum,maximum),enabled:maximum>=minimum,leadingMinimum:leadingMinimum};
}
function previewWidth(requested,available,scale){const limits=previewLimits(available,scale);return limits.enabled?Math.max(limits.minimum,Math.min(limits.maximum,Number.isInteger(requested)?requested:240)):0;}
function columnWidth(requested,viewport,scale){const minimum=Math.ceil(200*Math.max(1,Math.min(1.5,Number.isFinite(scale)?scale:1)));return Math.max(minimum,Math.min(Math.max(minimum,Math.min(600,Math.floor(viewport))),Number.isInteger(requested)?requested:240));}
if(typeof module!=="undefined")module.exports={sidebarMaximum,sidebarWidth,previewLimits,previewWidth,columnWidth};
