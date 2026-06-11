fn main() {
    // tauri.conf.json bundles target/release/diff-drift-cli.exe (the console
    // CLI) into the installers, and tauri-build validates that every bundle
    // resource exists while THIS script runs — before cargo has compiled the
    // bins of this same package. Pre-create an empty placeholder so clean
    // builds (cargo test, CI smoke) pass validation; the real exe overwrites
    // it during the release build, before the bundler collects resources.
    // Assumes the default cargo target dir, which is what `tauri build` and
    // release CI use.
    #[cfg(windows)]
    {
        let cli = std::path::Path::new("target/release/diff-drift-cli.exe");
        if !cli.exists() {
            if let Some(dir) = cli.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(cli, b"");
        }
    }
    tauri_build::build()
}
