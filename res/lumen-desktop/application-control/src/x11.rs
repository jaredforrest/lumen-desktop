//! Resolve the X11 connection's kernel peer PID through XRes, never _NET_WM_PID.
use std::ffi::c_void;
#[repr(C)] struct Spec { client:u32, mask:u32 }
#[repr(C)] struct Cookie { sequence:u32 }
#[repr(C)] struct Id { spec:Spec, length:u32 }
#[repr(C)] struct Iter { data:*mut Id, rem:i32, index:i32 }
#[link(name="xcb")]
unsafe extern "C" {
    fn xcb_connect(display:*const i8, screen:*mut i32)->*mut c_void;
    fn xcb_connection_has_error(connection:*mut c_void)->i32;
    fn xcb_disconnect(connection:*mut c_void);
}
#[link(name="xcb-res")]
unsafe extern "C" {
    fn xcb_res_query_client_ids(connection:*mut c_void, count:u32, specs:*const Spec)->Cookie;
    fn xcb_res_query_client_ids_reply(connection:*mut c_void,cookie:Cookie,error:*mut *mut c_void)->*mut c_void;
    fn xcb_res_query_client_ids_ids_iterator(reply:*const c_void)->Iter;
    fn xcb_res_client_id_value_value(value:*const Id)->*mut u32;
    fn xcb_res_client_id_value_value_length(value:*const Id)->i32;
}
pub fn peer(window:u32)->Result<u32,String> {
    let display=std::env::var("DISPLAY").unwrap_or_default();
    let local=display.strip_prefix(':').is_some_and(|value|!value.is_empty() && value.split('.').count()<=2 && value.split('.').all(|part|!part.is_empty() && part.bytes().all(|byte|byte.is_ascii_digit())));
    if !local{return Err("Force Quit requires the local XWayland display".into());}
    // XCB owns the connection and reply allocations until the paired cleanup.
    unsafe {
        let connection=xcb_connect(std::ptr::null(),std::ptr::null_mut());
        if connection.is_null(){return Err("Cannot connect to the application's X server".into());}
        let result=(||{
            if xcb_connection_has_error(connection)!=0{return Err("Cannot connect to the application's X server".into());}
            let cookie=xcb_res_query_client_ids(connection,1,&Spec{client:window,mask:2});
            let reply=xcb_res_query_client_ids_reply(connection,cookie,std::ptr::null_mut());
            if reply.is_null(){return Err("XRes could not verify the application's process".into());}
            let ids=xcb_res_query_client_ids_ids_iterator(reply);
            let pid=if ids.rem==1 && !ids.data.is_null() && (*ids.data).spec.mask==2 && xcb_res_client_id_value_value_length(ids.data)==1 {
                Some(*xcb_res_client_id_value_value(ids.data))
            }else{None};
            libc::free(reply);
            pid.filter(|pid|*pid>=2).ok_or_else(||"XRes could not verify the application's process".into())
        })();
        xcb_disconnect(connection);
        result
    }
}
