fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("src/icon.ico");
    res.compile().expect("Failed to compile Windows resources");
}
