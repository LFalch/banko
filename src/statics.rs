use crate::*;

#[get("/static/<file..>")]
pub fn static_handler(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
}

#[get("/favicon.ico")]
pub fn favicon_handler() -> Option<NamedFile> {
    static_handler(PathBuf::new().join("favicon.ico"))
}

#[get("/robots.txt")]
pub fn robots_handler() -> Option<NamedFile> {
    static_handler(PathBuf::new().join("robots.txt"))
}
