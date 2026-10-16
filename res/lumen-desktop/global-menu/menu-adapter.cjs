// Opt-in adapter: reads the app's actual menu and invokes its actual MenuItems.
const { app, Menu, BrowserWindow, webContents } = require('electron');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const root = process.env.CODEX_MENU_ADAPTER_ROOT;
if (root && process.env.SWAYSOCK) app.whenReady().then(() => {
  const child = spawn(root + '/python', [root + '/export-helper.py'], {stdio:['pipe','pipe','inherit']});
  let actions = new Map(), previous = '', alive = true;
  const hidden = new Map();
  const styles = new Map();
  let exporting = false;
  // This app has a React/Radix menubar in addition to Electron's native menu.
  // IDs come from its packaged application-menu component; never hide toolbars.
  const menuCSS = '[id^="application-menu-trigger-"], [role="menubar"]:has([id^="application-menu-trigger-"]), #application-menu-content, #application-menu-content-anchor { display:none !important; }';
  function appPage(wc) {
    const url=wc.getURL();
    return url.startsWith('app://') || (process.env.CODEX_MENU_RENDERER_TEST === '1' && url.startsWith('data:text/html'));
  }
  function syncStyle(wc) {
    if(wc.isDestroyed()) {styles.delete(wc.id);return;}
    if(!exporting || !appPage(wc)) return;
    if(styles.has(wc.id)) return;
    const entry={wc,key:null};styles.set(wc.id,entry);
    wc.insertCSS(menuCSS).then(key=>{
      entry.key=key;
      if(!exporting || styles.get(wc.id)!==entry) wc.removeInsertedCSS(key).catch(()=>{});
    }).catch(e=>{styles.delete(wc.id);console.error('Menu adapter CSS:',e.message);});
  }
  function clearStyles() {
    for(const entry of styles.values()) if(entry.key && !entry.wc.isDestroyed()) entry.wc.removeInsertedCSS(entry.key).catch(()=>{});
    styles.clear();
  }
  function watchContents(wc) {
    wc.on('did-finish-load',()=>{styles.delete(wc.id);syncStyle(wc);});
    wc.on('destroyed',()=>styles.delete(wc.id));
    syncStyle(wc);
  }
  app.on('web-contents-created',(_event,wc)=>watchContents(wc));
  for(const wc of webContents.getAllWebContents())watchContents(wc);
  function restore(){exporting=false;clearStyles();for(const [w,wasVisible] of hidden){if(!w.isDestroyed())w.setMenuBarVisibility(wasVisible);}hidden.clear();}
  function maintainHidden() {
    for (const w of hidden.keys()) {
      if (w.isDestroyed()) {hidden.delete(w);continue;}
      if (w.isMenuBarVisible()) w.setMenuBarVisibility(false);
    }
  }
  // Codex rebuilds its application menu after startup and on state changes.
  const setApplicationMenu = Menu.setApplicationMenu;
  Menu.setApplicationMenu = function(...args) {
    const result = setApplicationMenu.apply(this,args);
    maintainHidden();
    return result;
  };
  const ids = new WeakMap(); let nextId = 1;
  function tree(menu, map) {
    return menu.items.map(item => {
      if (!ids.has(item)) ids.set(item, nextId++);
      const id = ids.get(item); map.set(id, item);
      const props = {label:item.label, enabled:item.enabled, visible:item.visible};
      if (item.type === 'separator') props.type = 'separator';
      if (item.type === 'checkbox' || item.type === 'radio') {
        props['toggle-type'] = item.type === 'radio' ? 'radio' : 'checkmark';
        props['toggle-state'] = item.checked ? 1 : 0;
      }
      if (item.accelerator) props.shortcut = [item.accelerator.replace(/CommandOrControl|CmdOrCtrl/gi,'Control').split('+')];
      if (item.submenu) props['children-display'] = 'submenu';
      return {id,props,children:item.submenu ? tree(item.submenu,map):[]};
    });
  }
  function publish() {
    if (!alive) return;
    maintainHidden();
    if(exporting) for(const wc of webContents.getAllWebContents()) {
      syncStyle(wc);

    }
    const menu = Menu.getApplicationMenu();
    const map = new Map(); const children = menu ? tree(menu,map) : [];
    const state = BrowserWindow.getAllWindows().filter(w=>!w.isDestroyed() && w.isVisible()).map(w=>({xid:w.getNativeWindowHandle().readUInt32LE(),menu:{id:0,children}}));
    actions=map; const data=JSON.stringify(state);
    if(data!==previous){child.stdin.write(data+'\n');previous=data;}
  }
  readline.createInterface({input:child.stdout}).on('line',line=>{
    try {
      const cmd=JSON.parse(line);
      if ('registered' in cmd) {
        if (!cmd.registered) restore();
        else {
          exporting=true;
          for(const wc of webContents.getAllWebContents())syncStyle(wc);
          const w=BrowserWindow.getAllWindows().find(w=>!w.isDestroyed() && w.getNativeWindowHandle().readUInt32LE()===cmd.xid);
          if(w && !hidden.has(w)){hidden.set(w,w.isMenuBarVisible());w.setMenuBarVisibility(false);}
        }
        return;
      }
      const win=BrowserWindow.getFocusedWindow();
      publish(); const item=actions.get(cmd.id);
      if(!win || win.getNativeWindowHandle().readUInt32LE()!==cmd.xid || !item || !item.enabled || !item.visible || item.submenu || item.type==='separator')return;
      item.click(item,win,{});publish();
    } catch(e){console.error('Menu adapter action:',e.message);}
  });
  const timer=setInterval(publish,250);
  function stop(){restore();alive=false;clearInterval(timer);child.stdin.destroy();}
  child.on('exit',stop);child.on('error',stop);child.stdin.on('error',stop);
  app.on('before-quit',()=>{stop();child.kill();});publish();
});
