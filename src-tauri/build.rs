fn main() {
    tauri_build::build();

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo:rerun-if-changed=src/capture.m");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Speech");

        cc::Build::new()
            .file("src/capture.m")
            .flag("-fobjc-arc")
            .compile("aibuddy_capture");

        build_swift_speech_engine();

        // sherpa-onnx + onnxruntime dylibs are bundled into Contents/Frameworks
        // (see tauri.conf.json bundle.macOS.frameworks). They're referenced via
        // @rpath, so the executable needs an rpath pointing there. Dev runs work
        // via cargo's injected dylib path; this rpath is for the bundled .app.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
}

/// Compile speech_analyzer.swift (SpeechAnalyzer engine, macOS 26+ at runtime)
/// into a static lib. Swift stdlib cannot be statically linked on Apple
/// platforms; the runtime dylibs come from the OS (/usr/lib/swift, ≥10.14.4).
fn build_swift_speech_engine() {
    println!("cargo:rerun-if-changed=src/speech_analyzer.swift");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    };
    let deployment =
        std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "15.0".to_string());

    let sdk = std::env::var("SDKROOT").unwrap_or_else(|_| {
        let out = std::process::Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .expect("xcrun --show-sdk-path failed");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    });

    let profile = std::env::var("PROFILE").unwrap_or_default();
    let opt_flags: &[&str] = if profile == "release" { &["-O"] } else { &["-Onone", "-g"] };

    let lib_path = format!("{out_dir}/libaibuddy_speech.a");
    let status = std::process::Command::new("xcrun")
        .arg("swiftc")
        .args([
            "-emit-library",
            "-static",
            "-parse-as-library",
            "-module-name",
            "AiBuddySpeech",
            "-target",
            &format!("{arch}-apple-macosx{deployment}"),
            "-sdk",
            &sdk,
        ])
        .args(opt_flags)
        .arg("src/speech_analyzer.swift")
        .arg("-o")
        .arg(&lib_path)
        .status()
        .expect("failed to run swiftc — Xcode CLT required");
    assert!(status.success(), "swiftc failed compiling speech_analyzer.swift");

    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=aibuddy_speech");

    // Search paths for the Swift runtime dylibs referenced via autolink entries.
    let target_info = std::process::Command::new("swift")
        .args(["-print-target-info"])
        .output()
        .expect("swift -print-target-info failed");
    let info: serde_json::Value =
        serde_json::from_slice(&target_info.stdout).expect("bad target info JSON");
    if let Some(paths) = info["paths"]["runtimeLibraryPaths"].as_array() {
        for p in paths {
            if let Some(p) = p.as_str() {
                println!("cargo:rustc-link-search=native={p}");
            }
        }
    }
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
}
