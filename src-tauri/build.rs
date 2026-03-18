fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        // Compile the Objective-C voice capture wrapper (AVAudioEngine + VoiceProcessingIO)
        cc::Build::new()
            .file("native/voice_capture.m")
            .flag("-fobjc-arc")
            .compile("voice_capture");

        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
    }

    tauri_build::build()
}
