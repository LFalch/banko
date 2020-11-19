#![feature(decl_macro, proc_macro_hygiene)]
#[allow(unused_imports)]
#[macro_use]
extern crate tera;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate diesel;
extern crate dotenv;
#[macro_use(c)]
extern crate cute;

use crate::schema::numbers;
use diesel::prelude::*;
use diesel::{Insertable, Queryable};
use lazy_static::*;
use rand::Rng;
use rocket::{
    get,
    http::{ContentType, Status},
    response::{Content, NamedFile, Response},
};
use rocket_contrib::databases::{database, diesel::SqliteConnection};

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tera::{Context, Tera, Value};

mod errors;
mod schema;
mod statics;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[database("sqlite")]
pub struct DbConn(SqliteConnection);

#[derive(Queryable, Serialize, Debug)]
struct Numbers {
    id: i32,
    number_drawn: i32,
    draw_date: i32,
}

#[derive(Deserialize, Insertable)]
#[table_name = "numbers"]
struct NewNumber {
    number_drawn: i32,
}

pub type Res<'a> = Result<Response<'a>, Status>;
pub type ContRes<'a> = Content<Res<'a>>;

fn da_genitive_filter(value: Value, _args: HashMap<String, Value>) -> tera::Result<Value> {
    let mut name = try_get_value!("genitiv", "value", String, value);
    match name.chars().last() {
        Some('s') | Some('x') | Some('z') => name.push('\''),
        _ => name.push('s'),
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
        Err(_) => Err(Status::InternalServerError),
    }
}

pub fn create_context(current_page: &str) -> Context {
    let mut c = BASE_CONTEXT.clone();
    c.insert("cur", &current_page);
    c
}

pub fn respond_page(page: &'static str, c: Context) -> ContRes<'static> {
    Content(
        ContentType::HTML,
        tera_render(&format!("pages/{}.html", page), c),
    )
}

fn numbers_drawn(conn: DbConn) -> Vec<Numbers> {
    numbers::table
        .order(numbers::columns::id.desc())
        .load::<Numbers>(&*conn)
        .unwrap()
}

#[get("/")]
pub fn draw<'a>(conn: DbConn) -> ContRes<'a> {
    let mut context = create_context("draw");
    let mut numbers = [[0; 10]; 9];
    let drawn = c![x.number_drawn as usize, for x in numbers_drawn(conn)];
    for y in 0..=9 {
        for x in 0..=9 {
            let num = (x * 10) + y + 1;
            if drawn.contains(&num) {
                numbers[x][y] = num;
            };
        }
    }
    context.insert("numbers", &numbers);
    context.insert("chrono_numbers", &drawn);
    respond_page("draw", context)
}

#[get("/add/<number>")]
fn add_number(number: i32, conn: DbConn) -> String {
    let drawn = c![x.number_drawn as usize, for x in numbers_drawn(conn)];
    let mut full_pool = Vec::new();
    for i in 1..=90 {
        full_pool.push(i)
    }
    let mut pool = Vec::<usize>::new();
    for &x in full_pool.iter() {
        if !drawn.contains(&x) {
            pool.push(x as usize);
        }
    }
    if number < 70 && number > 0 {
        let mut rng = rand::thread_rng();
        // FIXME: It's possible to draw the same number multiple times
        let numbers: Vec<usize> = (0..number).map(|_| rng.gen_range(1, pool.len())).collect();
        // FIXME: ValuasClause
        for &number in numbers.iter() {
            let new_number = NewNumber {
                number_drawn: number as i32,
            };
            let result = diesel::insert_into(numbers::table).values(new_number);
            println!("Result: {:?}", result);
        }
    } else {
        return format!("{} is an invalid number!", number);
    }
    format!("Added {} numbers to the list!", number)
}

#[get("/list")]
fn get_numbers(conn: DbConn) -> String {
    format!("{:?}", numbers_drawn(conn))
}

fn main() {
    use crate::errors::*;
    rocket::ignite()
        .attach(DbConn::fairing())
        .mount(
            "/",
            routes![
                draw,
                add_number,
                get_numbers,
                crate::statics::robots_handler,
                crate::statics::favicon_handler,
                crate::statics::static_handler
            ],
        )
        .register(catchers![page_not_found, bad_request, server_error])
        .launch();
}
