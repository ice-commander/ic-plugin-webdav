mod dav;
pub mod mount;
use ic_plugin_api::{
    check_host, needs_plugin_assets, HostCheck, IcHost, IC_ABI_VERSION, IC_ERR_HOST_TOO_OLD,
    IC_ERR_HOST_UNKNOWN, IC_ERR_INIT_FAILED,
};
use std::ffi::CString;
use std::os::raw::{c_char, c_int};

pub const KIND: &str = "webdav";
pub const DOCUMENT: &str = include_str!("../documents/webdav.json");
pub const ID: &str = "ic-webdav-fs";
pub const ICON: &str = "webdav.svg";
pub const PICTURE: &[u8] = include_bytes!("../assets/webdav.svg");

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../version.rs"));

ic_plugin_api::declare_about!(
    "ic-webdav-fs",
    "WebDAV",
    plugins_version!(),
    "WebDAV connections"
);

#[cfg_attr(feature = "export-abi", no_mangle)]
pub extern "C" fn ic_plugin_init(host: *const IcHost, _kind: *const c_char) -> c_int {
    match check_host(host, IC_ABI_VERSION, needs_plugin_assets()) {
        HostCheck::Ok => {}
        HostCheck::WrongMagic => return IC_ERR_HOST_UNKNOWN,
        HostCheck::TooOld { .. } | HostCheck::Truncated { .. } => return IC_ERR_HOST_TOO_OLD,
    }
    let Ok(id) = CString::new(KIND) else {
        return IC_ERR_INIT_FAILED;
    };
    if let (Ok(owner), Ok(name)) = (CString::new(ID), CString::new(ICON)) {
        unsafe {
            ((*host).register_plugin_asset)(
                owner.as_ptr(),
                name.as_ptr(),
                PICTURE.as_ptr(),
                PICTURE.len() as u64,
            )
        };
    }
    let fs = mount::vtable();
    let connection = mount::connection_vtable(&fs);
    unsafe {
        ((*host).register_connection_kind)(
            id.as_ptr(),
            DOCUMENT.as_ptr(),
            DOCUMENT.len() as u64,
            &connection,
            std::ptr::null_mut(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_mount_refuses_settings_that_name_no_server() {
        for settings in ["{}", r#"{"user":"ivan"}"#, r#"{"url":""}"#] {
            let handle = mount::open(
                settings.as_ptr(),
                settings.len() as u64,
                std::ptr::null_mut(),
            );
            assert!(handle.is_null(), "opened on {settings}");
        }
    }

    #[test]
    fn a_mount_keeps_the_address_the_form_collected() {
        let settings = r#"{"url":"https://dav.example.org/remote.php/dav","user":"ivan","pass":"x","remote_path":"Documents"}"#;
        let handle = mount::open(
            settings.as_ptr(),
            settings.len() as u64,
            std::ptr::null_mut(),
        );
        assert!(!handle.is_null(), "a complete record opens");
        mount::close(handle);
    }

    #[test]
    fn the_filesystem_side_offers_every_operation_a_server_supports() {
        let table = mount::vtable();
        assert!(table.write.is_some());
        assert!(table.create_dir.is_some());
        assert!(table.remove.is_some());
        assert!(table.rename.is_some());
        assert_eq!(
            mount::is_read_only(std::ptr::null_mut()),
            0,
            "a webdav server is writable unless it refuses"
        );
    }

    #[test]
    fn the_plugin_refuses_a_host_it_does_not_recognise() {
        assert_eq!(
            ic_plugin_init(std::ptr::null(), std::ptr::null()),
            IC_ERR_HOST_UNKNOWN
        );
    }

    #[test]
    fn a_failure_is_handed_over_once_and_then_forgotten() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let settings = format!(r#"{{"url":"http://127.0.0.1:{port}/"}}"#);
        let handle = mount::open(settings.as_ptr(), settings.len() as u64, std::ptr::null_mut());
        assert!(!handle.is_null());
        assert!(mount::last_error(handle).is_null(), "nothing has failed yet");
        assert_eq!(mount::list(handle, c"/".as_ptr()).count, 0);
        assert!(!mount::last_error(handle).is_null(), "the failed listing gave no reason");
        assert!(mount::last_error(handle).is_null(), "the same failure was reported twice");
        mount::close(handle);
    }
}
