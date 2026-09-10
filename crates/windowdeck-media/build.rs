use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=native");
    for name in [
        "FrameExchange.h",
        "GpuFrames.h",
        "FramePacing.h",
        "RenderAdapter.h",
    ] {
        println!("cargo:rerun-if-changed=../../driver/windows-idd/{name}");
    }
    for name in ["WINDOWDECK_FFMPEG_DIR", "WINDOWDECK_SDL_DIR"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").file("native/media.cpp");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let ffmpeg = PathBuf::from(env::var_os("WINDOWDECK_FFMPEG_DIR").expect(
            "Set WINDOWDECK_FFMPEG_DIR to the shared FFmpeg SDK; see scripts/build-media.ps1",
        ));
        let sdl = PathBuf::from(
            env::var_os("WINDOWDECK_SDL_DIR")
                .expect("Set WINDOWDECK_SDL_DIR to the SDL2 development SDK"),
        );
        build
            .include(ffmpeg.join("include"))
            .include(sdl.join("include"));
        build.file("native/encoder.cpp").define("NOMINMAX", None);
        build.flag_if_supported("/EHsc");
        println!(
            "cargo:rustc-link-search=native={}",
            ffmpeg.join("lib").display()
        );
        println!(
            "cargo:rustc-link-search=native={}",
            sdl.join("lib/x64").display()
        );
        for lib in [
            "avcodec", "avutil", "swscale", "SDL2", "d3d11", "dxgi", "gdi32",
        ] {
            println!("cargo:rustc-link-lib={lib}");
        }
    } else {
        for name in ["libavcodec", "libavutil", "libswscale", "sdl2"] {
            let lib = pkg_config::Config::new()
                .probe(name)
                .expect("Install FFmpeg and SDL2 development packages");
            for path in lib.include_paths {
                build.include(path);
            }
        }
    }
    build.warnings(true).compile("windowdeck_media");
}
