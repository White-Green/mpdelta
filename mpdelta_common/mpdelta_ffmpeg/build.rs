fn main() {
    println!("cargo:rustc-link-lib=mfplat");
    println!("cargo:rustc-link-lib=strmiids");
    println!("cargo:rustc-link-lib=mfuuid");
}
