#![deny(missing_docs)]

//! mac_notification_sys requires to link extra libraries, add them here
fn main() {
    // Check if the target is macOS
    if cfg!(target_os = "macos") {
        // Link the AppKit framework (contains NSImage)
        println!("cargo:rustc-link-lib=framework=AppKit");
        // Link the CoreServices framework (contains LaunchServices functions)
        println!("cargo:rustc-link-lib=framework=CoreServices");
    }
}

