#![feature(decl_macro, proc_macro_hygiene)]
#[macro_use]
extern crate tera;
#[macro_use]
extern crate rocket;
// #[macro_use] extern crate rocket_contrib;
// #[macro_use] extern crate diesel;

use std::{
    path::{Path, PathBuf},
    collections::HashMap,
};
use rocket::{
    response::{Content, NamedFile, Response},
    http::{Status, ContentType}
};
use tera::{Tera, Context, Value};
use lazy_static::*;

mod errors;
mod statics;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub type Res<'a> = Result<Response<'a>, Status>;
pub type ContRes<'a> = Content<Res<'a>>;

fn da_genitive_filter(value: Value, _args: HashMap<String, Value>) -> tera::Result<Value> {
    let mut name = try_get_value!("genitiv", "value", String, value);
    match name.chars().last() {
        Some('s') | Some('x') | Some('z') => name.push('\''),
        _ => name.push('s')
    }
    Ok(Value::String(name))
}

fn abs_filter(value: Value, _: HashMap<String, Value>) -> tera::Result<Value> {
    let num = try_get_value!("abs", "value", i32, value);
    Ok(num.abs().into())
}

lazy_static! {
    static ref TERA: Tera = {
        let mut tera = compile_templates!("templates/**/*");
        tera.autoescape_on(vec![]);
        tera.register_filter("abs", abs_filter);
        tera.register_filter("genitiv", da_genitive_filter);
        tera
    };
    static ref BASE_CONTEXT: Context = {
        let mut c = Context::new();
        c.insert("version", &VERSION);
        c
    };
}

pub fn tera_render(template: &str, c: Context) -> Res<'static> {
    use std::io::Cursor;
    match TERA.render(template, &c) {
        Ok(s) => Response::build().sized_body(Cursor::new(s)).ok(),
        Err(_) => Err(Status::InternalServerError)
    }
}

pub fn create_context(current_page: &str) -> Context {
    let mut c = BASE_CONTEXT.clone();
    c.insert("cur", &current_page);
    c
}

pub fn respond_page(page: &'static str, c: Context) -> ContRes<'static> {
    Content(ContentType::HTML, tera_render(&format!("pages/{}.html", page), c))
}

#[get("/")]
pub fn root<'a>() -> ContRes<'a> {
    respond_page("root", create_context("root"))
}

fn main() {
    use crate::errors::*;
    rocket::ignite()
        .mount("/", routes![
            root,
            crate::statics::robots_handler,
            crate::statics::favicon_handler,
            crate::statics::static_handler
        ])
        .register(catchers![page_not_found, bad_request, server_error])
        .launch();
}
