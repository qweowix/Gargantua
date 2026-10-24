fn main() {
    // UI-строки на русском в @tr(), английский — lang/en/LC_MESSAGES/blackhole.po (вшивается в бинарник)
    let cfg = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    println!("cargo:rerun-if-changed=lang");
    slint_build::compile_with_config("ui/app.slint", cfg).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        let _ = res.compile();
    }
}
