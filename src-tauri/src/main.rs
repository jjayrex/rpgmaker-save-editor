// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    work_around_nvidia_blank_window();

    rpgmaker_save_editor_lib::run()
}

/// Turns off WebKitGTK's DMA-BUF renderer when the proprietary NVIDIA driver is
/// in use, because the two cannot share graphics buffers and the window comes
/// up blank:
///
/// ```text
/// KMS: DRM_IOCTL_MODE_CREATE_DUMB failed: Permission denied
/// Failed to create GBM buffer of size 1180x780: Permission denied
/// ```
///
/// The alternative renderer costs nothing noticeable for an interface like this
/// one, and an empty window is not something anyone should have to diagnose.
/// Setting `WEBKIT_DISABLE_DMABUF_RENDERER` yourself — to any value — overrides
/// this, so the renderer can be forced back on.
#[cfg(target_os = "linux")]
fn work_around_nvidia_blank_window() {
    const FLAG: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

    if std::env::var_os(FLAG).is_some() {
        return;
    }
    // These exist only for the proprietary driver; nouveau does not have the
    // problem and does not create them.
    let nvidia_in_use = ["/proc/driver/nvidia/version", "/sys/module/nvidia/version"]
        .iter()
        .any(|path| std::path::Path::new(path).exists());
    if !nvidia_in_use {
        return;
    }

    eprintln!("NVIDIA driver detected: disabling WebKit's DMA-BUF renderer, which it cannot use.");
    // Safe: nothing else has started a thread yet, this being the first thing
    // `main` does.
    unsafe { std::env::set_var(FLAG, "1") };
}
