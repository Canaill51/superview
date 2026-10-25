//! What the host asks first: how many plugins, and which.

use std::ffi::CStr;

unsafe extern "C" {
    fn OfxGetNumberOfPlugins() -> i32;
    fn OfxGetPlugin(nth: i32) -> *const ofx_plugin::OfxPlugin;
}

mod ofx_plugin {
    use std::os::raw::{c_char, c_uint};
    #[repr(C)]
    pub struct OfxPlugin {
        pub plugin_api: *const c_char,
        pub api_version: c_int,
        pub plugin_identifier: *const c_char,
        pub plugin_version_major: c_uint,
        pub plugin_version_minor: c_uint,
    }
    use std::os::raw::c_int;
}

#[test]
fn the_host_finds_one_effect_named_after_the_project() {
    // The crate is linked for its exports; naming it keeps the linker from dropping them.
    let _ = superview_ofx::warp::Mode::Widen;
    unsafe {
        assert_eq!(OfxGetNumberOfPlugins(), 1);
        let plugin = &*OfxGetPlugin(0);
        assert_eq!(
            CStr::from_ptr(plugin.plugin_api).to_str().unwrap(),
            "OfxImageEffectPluginAPI"
        );
        assert_eq!(
            CStr::from_ptr(plugin.plugin_identifier).to_str().unwrap(),
            "io.github.canaill51.superview"
        );
    }
}
