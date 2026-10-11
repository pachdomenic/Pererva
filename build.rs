fn main() {
    // Тексти розмітки — українською; переклади лежать у lang/<мова>/LC_MESSAGES.
    // Без контексту перекладу: однаковий текст у різних компонентах
    // перекладається однаково.
    let config = slint_build::CompilerConfiguration::new()
        .with_style("material".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("не вдалося скомпілювати інтерфейс Slint");
    println!("cargo:rerun-if-changed=lang");
}
