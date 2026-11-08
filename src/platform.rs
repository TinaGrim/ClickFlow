pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        unsafe extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }

        // SAFETY: AXIsProcessTrusted has no inputs and only reads the current
        // process trust state.
        unsafe { AXIsProcessTrusted() != 0 }
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open System Settings: {error}"))
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Accessibility settings are only required on macOS.".to_owned())
    }
}
