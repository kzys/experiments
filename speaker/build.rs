//! Links ESP-SR's wake word libraries (prebuilt, from `vendor/`, see
//! `fetch-esp-sr.sh`) and packs the wake word models into one image.

use std::{env, fs, path::Path, path::PathBuf, process::Command};

/// Wake word models to pack, from ESP-SR's `model/wakenet_model/`. Only
/// WakeNet9 models: WakeNet10 needs esp-dl, which is not linked.
const MODELS: &[&str] = &["wn9_hiesp", "wn9_computer_tts", "wn9_alexa"];

fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    let vendor = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("vendor");
    let sr = vendor.join("esp-sr");
    if !sr.exists() {
        panic!("{} is missing; run ./fetch-esp-sr.sh", sr.display());
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=vendor");

    let dl_fft = vendor.join("dl_fft");
    let mut c = cc::Build::new();
    c.compiler("xtensa-esp32s3-elf-gcc")
        .archiver("xtensa-esp32s3-elf-ar")
        .cargo_metadata(false)
        .opt_level(2)
        .flag("-mlongcalls")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-ffast-math")
        .warnings(false)
        .define("CONFIG_IDF_TARGET_ESP32S3", "1")
        .include("csrc/include")
        .include(&dl_fft)
        .include(dl_fft.join("base"))
        .include(dl_fft.join("base/isa"))
        .file("csrc/sr_glue.c")
        .file(vendor.join("cjson/cJSON/cJSON.c"));
    for f in [
        "dl_fft_f32.c",
        "dl_fft_s16.c",
        "dl_rfft_f32.c",
        "dl_rfft_s16.c",
        "dl_fft_s32.c",
        "dl_rfft_s32.c",
    ] {
        c.file(dl_fft.join(f));
    }
    for f in [
        "dl_fft2r_fc32_ansi.c",
        "dl_fft4r_fc32_ansi.c",
        "dl_fft2r_sc16_ansi.c",
        "dl_fft2r_sc16_dif_ansi.c",
        "dl_fft2r_sc32_ansi.c",
        "dl_fft_base.c",
        "dl_fft_table.c",
        "isa/esp32s3/dl_fft2r_fc32_aes3.S",
        "isa/esp32s3/dl_fft4r_fc32_aes3.S",
        "isa/esp32s3/dl_fft2r_sc16_aes3.S",
    ] {
        c.file(dl_fft.join("base").join(f));
    }
    c.compile("sr_glue");

    // One group, so the libraries and the glue can refer to each other in
    // any order. The glue's other half is Rust (src/sr.rs). newlib provides
    // the string, number and printf functions.
    println!("cargo:rustc-link-search=native={}", out.display());
    for lib in ["wakenet", "dl_lib", "c_speech_features", "hufzip"] {
        let name = format!("lib{lib}.a");
        keep_literals_near(&sr.join("lib/esp32s3").join(&name), &out.join(&name));
    }
    let glue = out.join("libsr_glue.a");
    keep_literals_near(&glue, &glue);
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for lib in [
        "sr_glue",
        "wakenet",
        "dl_lib",
        "c_speech_features",
        "hufzip",
        "c",
        "m",
    ] {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");

    let image = pack_models(&sr.join("model/wakenet_model"), MODELS);
    fs::write(out.join("srmodels.bin"), image).unwrap();
}

/// Copies archive `from` to `to`, renaming bare `.literal` sections (from
/// assembly) to `.literal.sr`. esp-hal's linker script puts all bare
/// `.literal` sections first, out of L32R reach of code far into the image;
/// renamed, they stay next to their object's code.
fn keep_literals_near(from: &Path, to: &Path) {
    let status = Command::new("xtensa-esp32s3-elf-objcopy")
        .args(["--rename-section", ".literal=.literal.sr"])
        .arg(from)
        .arg(to)
        .status()
        .expect("xtensa-esp32s3-elf-objcopy");
    assert!(status.success(), "objcopy {}", from.display());
}

/// Packs models the way ESP-SR's `model/pack_model.py` does: the model
/// count, then per model its name, file count and per file its name,
/// offset and length, then all files' data.
fn pack_models(dir: &Path, models: &[&str]) -> Vec<u8> {
    let mut entries = Vec::new();
    for m in models {
        let mut files: Vec<_> = fs::read_dir(dir.join(m))
            .unwrap_or_else(|e| panic!("model {m}: {e}"))
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        entries.push((m, files));
    }
    let file_count: usize = entries.iter().map(|(_, f)| f.len()).sum();
    let header_len = 4 + entries.len() * (32 + 4) + file_count * (32 + 4 + 4);
    let mut head = Vec::new();
    let mut data = Vec::new();
    head.extend((entries.len() as u32).to_le_bytes());
    for (name, files) in entries {
        head.extend(name32(name));
        head.extend((files.len() as u32).to_le_bytes());
        for f in files {
            let bytes = fs::read(&f).unwrap();
            head.extend(name32(f.file_name().unwrap().to_str().unwrap()));
            head.extend(((header_len + data.len()) as u32).to_le_bytes());
            head.extend((bytes.len() as u32).to_le_bytes());
            data.extend(bytes);
        }
    }
    assert_eq!(head.len(), header_len);
    head.extend(data);
    head
}

/// A name in a 32-byte field, padded with NULs.
fn name32(s: &str) -> [u8; 32] {
    assert!(s.len() <= 32, "name too long: {s}");
    let mut b = [0; 32];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}
