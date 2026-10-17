fn main() {
    // The window tests find elements through `ElementHandle`, which the testing backend can
    // only walk when the generated code carries debug info (measured: without it, every
    // search comes back empty). The release binary does not need it and is built without.
    let debug_info = std::env::var("PROFILE").is_ok_and(|profile| profile != "release");
    // The window icon is `cmd/superview/Icon.png`, the file the Go build reads too. It is
    // embedded in the binary: by default Slint would look for it at its build-machine path when
    // the program runs. (`EmbedForSoftwareRenderer`, which decodes at build time, also embeds
    // the glyphs and does not compile.)
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(debug_info)
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("failed to compile ui/main.slint");
}
