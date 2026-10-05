fn main() {
    println!("cargo:rerun-if-changed=assets/lolite.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/lolite.ico");
    res.compile().expect("embedding icon (needs Windows SDK rc.exe)");
}
