use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS is available");
    let library = match target_os.as_str() {
        "linux" => "libduckdb.so",
        "macos" => "libduckdb.dylib",
        "windows" => "duckdb.dll",
        _ => panic!("Prebuilt DuckDB is not supported on {target_os}"),
    };
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory is available"));
    let build_dir = out_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("Cargo build directory");
    assert_eq!(
        build_dir.file_name().and_then(|name| name.to_str()),
        Some("build")
    );
    let profile = build_dir.parent().expect("Cargo profile directory");
    let source = profile.join("deps").join(library);
    let destination = profile.join(library);
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", destination.display());
    let source_metadata = fs::metadata(&source)?;
    let copy = fs::metadata(&destination).map_or(true, |metadata| {
        metadata.len() != source_metadata.len()
            || metadata.modified().ok() < source_metadata.modified().ok()
    });
    if copy {
        // Keep the library next to both binaries. On Unix, replace it atomically
        // so an already running application retains its previous loaded library.
        if target_os == "windows" {
            fs::copy(&source, &destination)?;
        } else {
            let temporary = out_dir.join(library);
            fs::copy(&source, &temporary)?;
            fs::rename(&temporary, &destination)?;
        }
    }
    match target_os.as_str() {
        "linux" => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        "macos" => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
    Ok(())
}
