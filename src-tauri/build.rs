fn main() {
    // No bundle config needed for the console CLI: tauri-cli picks up every
    // [[bin]] in this crate (diff-drift and diff-drift-cli) and the bundler
    // installs them side by side. Listing the CLI in bundle.resources too
    // would install it twice — WiX rejects that (ICE30) and the MSI fails.
    tauri_build::build()
}
