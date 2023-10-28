use std::env;
fn main() {
    if let Ok(_) = env::var("CARGO_CFG_WINDOWS") {
        embed_resource::compile("resources.rc", embed_resource::NONE);
    }

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
}
