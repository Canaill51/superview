fn main() {
    // The window tests find elements through `ElementHandle`, which the testing backend can
    // only walk when the generated code carries debug info (measured: without it, every
    // search comes back empty). The release binary does not need it and is built without.
    let debug_info = std::env::var("PROFILE").is_ok_and(|profile| profile != "release");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_info);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("failed to compile ui/main.slint");
}
