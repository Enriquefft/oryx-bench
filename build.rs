fn main() {
    // When built inside the Nix dev shell, bake an rpath into the
    // binary so it finds wayland / libGL / xorg libs at runtime
    // without needing LD_LIBRARY_PATH. The flake's shellHook exports
    // ORYX_RUNTIME_RPATH as a colon-separated list of store paths.
    // Outside the dev shell this is unset and the build falls back
    // to the linker's default behavior — no change for distro users.
    println!("cargo:rerun-if-env-changed=ORYX_RUNTIME_RPATH");
    if let Ok(rpath) = std::env::var("ORYX_RUNTIME_RPATH") {
        for path in rpath.split(':').filter(|p| !p.is_empty()) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
        }
    }
}
