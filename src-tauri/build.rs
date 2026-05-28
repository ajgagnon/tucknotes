fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        // Compile the Objective-C native helpers (mic capture + output-device detection).
        cc::Build::new()
            .file("native/voice_capture.m")
            .file("native/audio_output.m")
            .flag("-fobjc-arc")
            .compile("voice_capture");

        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
    }

    tauri_build::build()
}
