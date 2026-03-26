fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winres::WindowsResource::new();
        res.set_icon("devoid.ico");
        res.set("ProductName", "Devoid Viewer");
        res.set("FileDescription", "Devoid Viewer");
        res.compile().unwrap();
    }
}