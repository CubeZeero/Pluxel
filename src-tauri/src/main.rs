// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // `pluxel package …` runs the headless CLI packager and exits without a window.
    if args.get(1).map(String::as_str) == Some("package") {
        // The release build is a GUI-subsystem app; attach to the parent console
        // so stdout/stderr are visible when launched from a terminal.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Console::AttachConsole(
                windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
        std::process::exit(pluxel_lib::cli::run(&args[2..]));
    }
    pluxel_lib::run()
}
