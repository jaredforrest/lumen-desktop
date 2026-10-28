//! Optional GTK/Wayland input-method-v2 bridge. The C layer owns the protocol role.
use std::cell::RefCell;
use std::ffi::{CStr,CString,c_char,c_int,c_void};
use std::rc::{Rc,Weak};

#[link(name="dl")]
unsafe extern "C" {
    fn dlopen(path:*const c_char,flags:c_int)->*mut c_void;
    fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;
    fn dlclose(handle:*mut c_void)->c_int;
}
const RTLD_NOW:c_int=2;

type Changed=extern "C" fn();
type Replaced=extern "C" fn(*const c_char,u32,u32,u32);
type Init=unsafe extern "C" fn(Changed)->c_int;
type SetReplacement=unsafe extern "C" fn(Replaced);
type Flag=unsafe extern "C" fn()->c_int;
type Epoch=unsafe extern "C" fn()->u32;
type Attach=unsafe extern "C" fn(*mut c_void)->c_int;
type Commit=unsafe extern "C" fn(*const c_char,u32)->c_int;
type Replace=unsafe extern "C" fn(u32,*const c_char,u32,u32)->c_int;
type Hide=unsafe extern "C" fn();

pub struct Bridge {
    handle:*mut c_void,
    init:Init,
    set_replacement:SetReplacement,
    active:Flag,
    available:Flag,
    positioned:Flag,
    epoch:Epoch,
    attach:Attach,
    commit:Commit,
    replace:Replace,
    hide:Hide,
}
impl Drop for Bridge {fn drop(&mut self) {unsafe {dlclose(self.handle);}}}

thread_local! {
    static CURRENT:RefCell<Option<Weak<Bridge>>>=const {RefCell::new(None)};
    static ON_CHANGE:RefCell<Option<Box<dyn Fn()>>>=const {RefCell::new(None)};
}

extern "C" fn changed() {
    ON_CHANGE.with(|callback| {if let Some(callback)=callback.borrow().as_ref(){callback();}});
}
extern "C" fn replacement(data:*const c_char,cursor:u32,epoch:u32,serial:u32) {
    if data.is_null() {return;}
    // The bridge only sends bounded, validated UTF-8 surrounding text.
    let bytes=unsafe {CStr::from_ptr(data)}.to_bytes();
    let Some(prefix)=bytes.get(..cursor as usize).and_then(|bytes|std::str::from_utf8(bytes).ok()) else {return};
    let Some((before,value))=character_replacements::replacement_for(prefix) else {return};
    if before>u32::MAX as usize || value.len()>16384 {return;}
    let Ok(value)=CString::new(value) else {return};
    CURRENT.with(|current| {if let Some(bridge)=current.borrow().as_ref().and_then(Weak::upgrade) {
        unsafe {(bridge.replace)(before as u32,value.as_ptr(),epoch,serial);}
    }});
}

unsafe fn symbol<T>(handle:*mut c_void,name:&str)->Result<T,String> {
    let name=CString::new(name).map_err(|error|error.to_string())?;
    let raw=unsafe {dlsym(handle,name.as_ptr())};
    if raw.is_null(){return Err(format!("Missing input bridge symbol: {}",name.to_string_lossy()));}
    // POSIX dlsym returns a function pointer on this supported Linux target.
    Ok(unsafe {std::mem::transmute_copy(&raw)})
}

impl Bridge {
    pub fn load()->Result<Rc<Self>,String> {
        let path=std::env::var("CHARACTER_VIEWER_BRIDGE").map_err(|_|"Input bridge path is unset".to_string())?;
        let path=CString::new(path).map_err(|error|error.to_string())?;
        let handle=unsafe {dlopen(path.as_ptr(),RTLD_NOW)};
        if handle.is_null(){return Err("Input bridge could not be loaded".into());}
        let load=|| -> Result<Self,String> {unsafe {Ok(Self {
            handle,
            init:symbol(handle,"cv_init")?,
            set_replacement:symbol(handle,"cv_replacement_callback")?,
            active:symbol(handle,"cv_active")?,
            available:symbol(handle,"cv_available")?,
            positioned:symbol(handle,"cv_positioned")?,
            epoch:symbol(handle,"cv_epoch")?,
            attach:symbol(handle,"cv_attach")?,
            commit:symbol(handle,"cv_commit")?,
            replace:symbol(handle,"cv_replace")?,
            hide:symbol(handle,"cv_hide")?,
        })}};
        let bridge=match load(){Ok(bridge)=>Rc::new(bridge),Err(error)=>{unsafe{dlclose(handle)};return Err(error);}};
        CURRENT.with(|current|*current.borrow_mut()=Some(Rc::downgrade(&bridge)));
        unsafe {(bridge.set_replacement)(replacement);(bridge.init)(changed);}
        Ok(bridge)
    }
    pub fn on_change(&self,callback:impl Fn()+'static) {ON_CHANGE.with(|slot|*slot.borrow_mut()=Some(Box::new(callback)));}
    pub fn active(&self)->bool {unsafe{(self.active)()!=0}}
    pub fn available(&self)->bool {unsafe{(self.available)()!=0}}
    pub fn positioned(&self)->bool {unsafe{(self.positioned)()!=0}}
    pub fn epoch(&self)->u32 {unsafe{(self.epoch)()}}
    pub fn attach(&self,widget:*mut c_void)->bool {unsafe{(self.attach)(widget)!=0}}
    pub fn hide(&self) {unsafe{(self.hide)();}}
    pub fn commit(&self,text:&str,epoch:u32)->bool {
        let Ok(text)=CString::new(text) else {return false};
        unsafe{(self.commit)(text.as_ptr(),epoch)!=0}
    }
}
