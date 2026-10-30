//! Exercise the actual patched upstream OpenSSL helper through its C ABI.
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    net::IpAddr,
    ptr,
};
#[link(name = "vncclient")]
unsafe extern "C" {
    fn lumen_vnc_set_peer_identity(parameter: *mut c_void, host: *const c_char) -> c_int;
    fn lumen_vnc_clipboard_text_length_valid(length: c_int) -> c_int;
    fn SendClientCutTextUTF8(client: *mut c_void, text: *const c_char, length: c_int) -> c_int;
}
#[link(name = "crypto")]
unsafe extern "C" {
    fn X509_VERIFY_PARAM_new() -> *mut c_void;
    fn X509_VERIFY_PARAM_free(parameter: *mut c_void);
    fn X509_VERIFY_PARAM_get0_host(parameter: *mut c_void, index: usize) -> *const c_char;
    fn X509_VERIFY_PARAM_get1_ip_asc(parameter: *mut c_void) -> *mut c_char;
    fn CRYPTO_free(pointer: *mut c_void, file: *const c_char, line: c_int);
}
fn verify() {
    unsafe {
        for (length, expected) in [
            (-1, 0),
            (0, 1),
            (1, 1),
            (c_int::MAX - 1, 1),
            (c_int::MAX, 0),
        ] {
            assert_eq!(lumen_vnc_clipboard_text_length_valid(length), expected);
        }
        // Invalid lengths are rejected before touching connection or text memory.
        for length in [-1, c_int::MAX] {
            assert_eq!(
                SendClientCutTextUTF8(ptr::null_mut(), ptr::null(), length),
                0
            );
        }
        for address in ["127.0.0.1", "192.0.2.19", "::1", "2001:db8::19"] {
            let parameter = X509_VERIFY_PARAM_new();
            assert!(!parameter.is_null());
            let text = CString::new(address).unwrap();
            assert_eq!(lumen_vnc_set_peer_identity(parameter, text.as_ptr()), 1);
            assert!(X509_VERIFY_PARAM_get0_host(parameter, 0).is_null());
            let actual = X509_VERIFY_PARAM_get1_ip_asc(parameter);
            assert!(!actual.is_null());
            assert_eq!(
                CStr::from_ptr(actual)
                    .to_str()
                    .unwrap()
                    .parse::<IpAddr>()
                    .unwrap(),
                address.parse::<IpAddr>().unwrap()
            );
            CRYPTO_free(actual.cast(), ptr::null(), 0);
            X509_VERIFY_PARAM_free(parameter);
        }
        let parameter = X509_VERIFY_PARAM_new();
        assert!(!parameter.is_null());
        let name = CString::new("fixture.example.test").unwrap();
        assert_eq!(lumen_vnc_set_peer_identity(parameter, name.as_ptr()), 1);
        assert_eq!(
            CStr::from_ptr(X509_VERIFY_PARAM_get0_host(parameter, 0)).to_bytes(),
            name.as_bytes()
        );
        assert!(X509_VERIFY_PARAM_get1_ip_asc(parameter).is_null());
        assert_eq!(lumen_vnc_set_peer_identity(parameter, c"".as_ptr()), 0);
        assert_eq!(lumen_vnc_set_peer_identity(parameter, ptr::null()), 0);
        assert_eq!(
            lumen_vnc_set_peer_identity(ptr::null_mut(), c"127.0.0.1".as_ptr()),
            0
        );
        X509_VERIFY_PARAM_free(parameter);
    }
}
fn main() {
    verify();
    println!("Actual upstream peer-identity verifier: IPv4/IPv6/DNS/invalid checks passed");
}
#[test]
fn actual_upstream_verifier() {
    verify();
}
