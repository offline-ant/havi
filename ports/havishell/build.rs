use std::path::Path;
use std::process::Command;

fn main() {
    let svg = Path::new("../../../hppr/logo.svg");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Generate 64x64 and 128x128 RGBA PNGs from the SVG at build time.
    for size in [64, 128] {
        let out_png = format!("{}/hppr_icon_{}.png", out_dir, size);
        let status = Command::new("magick")
            .args([
                svg.to_str().unwrap(),
                "-resize",
                &format!("{}x{}", size, size),
                "-background",
                "none",
                "-flatten",
                &format!("PNG32:{}", out_png),
            ])
            .status()
            .expect("failed to run magick (ImageMagick) to convert icon SVG");
        assert!(status.success(), "magick icon conversion failed");
    }

    println!("cargo:rerun-if-changed={}", svg.display());
}
