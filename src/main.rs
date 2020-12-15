#![feature(decl_macro, proc_macro_hygiene)]
#[macro_use]
extern crate tera;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate diesel;
#[macro_use(c)]
extern crate cute;
#[macro_use]
extern crate dotenv_codegen;

use crate::schema::{numbers, winner};
use diesel::prelude::*;
use diesel::{Insertable, Queryable, QueryableByName};
use dotenv::dotenv;
use lazy_static::*;
use lettre::message::{header, SinglePart};
use lettre::{transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use rand::seq::SliceRandom;
use rocket::{
    get,
    http::{ContentType, RawStr, Status},
    post,
    request::{self, Form, FromFormValue, FromRequest},
    response::{Content, NamedFile, Redirect, Response},
    Outcome, Request,
};
use rocket_contrib::databases::{database, diesel::SqliteConnection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};
use tera::{Context, Tera, Value};

mod errors;
mod schema;
mod statics;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[database("sqlite")]
pub struct DbConn(SqliteConnection);

type Session<'a> = rocket_session::Session<'a, Vec<String>>;

struct IgnoreField;

impl<'a> FromFormValue<'a> for IgnoreField {
    type Error = &'a str;

    fn from_form_value(_: &'a RawStr) -> Result<Self, Self::Error> {
        Ok(IgnoreField)
    }

    fn default() -> Option<Self> {
        Some(IgnoreField)
    }
}

#[derive(FromForm)]
struct UserLogin {
    username: String,
    password: String,
    #[allow(dead_code)]
    submit: IgnoreField,
}

#[derive(FromForm)]
struct Banko {
    name: String,
    how: i32,
    #[allow(dead_code)]
    submit: IgnoreField,
}

#[derive(Queryable, Serialize, QueryableByName, Debug)]
#[table_name = "numbers"]
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

#[derive(Queryable, QueryableByName, Debug, Serialize)]
#[table_name = "winner"]
struct Winner {
    id: i32,
    name: String,
    how: i32,
    when: String,
}

#[derive(Deserialize, Insertable)]
#[table_name = "winner"]
struct NewWinner {
    name: String,
    how: i32,
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

fn numbers_drawn(conn: &DbConn) -> Vec<Numbers> {
    numbers::table
        .order(numbers::columns::id.asc())
        .load::<Numbers>(&**conn)
        .unwrap()
}

fn numbers_drawn_today(conn: &DbConn) -> Vec<Numbers> {
    diesel::sql_query(
        "select * from numbers where substr(draw_date, 0,11)
        LIKE date('now', 'localtime') order by id asc",
    )
    .load::<Numbers>(&**conn)
    .unwrap()
}

fn winner_claim_add(conn: &DbConn, name: String, how: i32) -> QueryResult<usize> {
    let new_winner = NewWinner { name, how };
    diesel::insert_into(winner::table)
        .values(new_winner)
        .execute(&**conn)
}

fn winner_claims(conn: &DbConn) -> Vec<Winner> {
    diesel::sql_query("select * from winner order by rowid DESC")
        .load::<Winner>(&**conn)
        .unwrap()
}

fn banko_notify(conn: &DbConn, name: String, how: i32) -> String {
    dotenv().ok();
    let how_string = match how {
        1 => "1 række.",
        2 => "2 rækker.",
        3 => "hele pladen!",
        _ => "snyderi!!!",
    };
    let body = format!("Hej!\n\n{} ansøger om gevinst for {}", name, how_string);
    let part = SinglePart::builder()
        .header(header::ContentType(
            "text/plain; charset=utf8".parse().unwrap(),
        ))
        .header(header::ContentTransferEncoding::Binary)
        .body(body);
    let subject = format!("{} har vundet", name);
    let email = Message::builder()
        .from("Julebanko <julebanko@fair-it.dk>".parse().unwrap())
        .to(
            format!("{} <{}>", dotenv!("ADMIN_NAME"), dotenv!("ADMIN_MAIL"))
                .parse()
                .unwrap(),
        )
        .subject(subject)
        .singlepart(part)
        .unwrap();
    let creds = Credentials::new(
        dotenv!("MAIL_USER").to_string(),
        dotenv!("MAIL_PASSWORD").to_string(),
    );
    let mailer = SmtpTransport::relay(dotenv!("MAIL_SERVER"))
        .unwrap()
        .credentials(creds)
        .build();
    match mailer.send(&email) {
        Ok(_) => {
            let _ = winner_claim_add(&conn, name, how);
            "ok".to_string()
        }
        Err(e) => format!("Could not send email: {:?}", e),
    }
}

pub fn add_number_to_db(number: i32, conn: &DbConn) -> QueryResult<usize> {
    let new_number = NewNumber {
        number_drawn: number as i32,
    };
    diesel::insert_into(numbers::table)
        .values(new_number)
        .execute(&**conn)
}

pub struct RemoteAddr {
    addr: String,
}

impl RemoteAddr {
    fn addr(self) -> String {
        self.addr
    }
}

impl<'a, 'r> FromRequest<'a, 'r> for RemoteAddr {
    type Error = ();

    fn from_request(req: &'a Request<'r>) -> request::Outcome<Self, ()> {
        if req.headers().contains("X-Forwarded-For") {
            let header = req.headers().get_one("X-Forwarded-For").unwrap();
            let addr = header.split(',').next().unwrap();

            return Outcome::Success(RemoteAddr {
                addr: addr.to_string(),
            });
        }

        Outcome::Success(RemoteAddr {
            addr: req.remote().unwrap().ip().to_string(),
        })
    }
}

#[get("/")]
pub fn draw<'a>(conn: DbConn, session: Session) -> ContRes<'a> {
    let mut context = create_context("draw");
    let mut session_user = String::new();
    session.tap(|sess| {
        for user in sess.iter().take(1) {
            session_user = user.to_owned();
        }
    });
    context.insert("login", &session_user);
    let mut numbers = [[0; 10]; 9];
    let drawn = c![x.number_drawn as usize, for x in numbers_drawn(&conn)];
    let drawn_today = c![x.number_drawn as usize, for x in numbers_drawn_today(&conn)];
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
    context.insert("drawn_today", &drawn_today);
    respond_page("draw", context)
}

#[get("/error")]
fn error<'a>(_session: Session, req: RemoteAddr) ->ContRes<'a> {
    let mut context = create_context("error");
    let remote_ip = req.addr();
    context.insert("remote_ip", &remote_ip);
    respond_page("error", context)
}


#[get("/add/<number>")]
fn add_number(number: usize, conn: DbConn, session: Session) -> String {
    let mut session_user = String::new();
    session.tap(|sess| {
        for user in sess.iter().take(1) {
            session_user = user.to_owned();
        }
    });
    if session_user == "admin" {
        let drawn = c![x.number_drawn, for x in numbers_drawn(&conn)];
        let mut pool = Vec::<i32>::new();
        let mut full_pool = Vec::new();
        for x in 1..=90 {
            full_pool.push(x)
        }
        for &x in full_pool.iter() {
            if !drawn.contains(&x) {
                pool.push(x);
            }
        }
        if number <= pool.len() && number > 0 {
            let mut rng = rand::thread_rng();
            for &numb in pool.choose_multiple(&mut rng, number) {
                add_number_to_db(numb as i32, &conn).unwrap();
            }
            format!("Added {} numbers to the list!", number)
        } else {
            format!("Cannot add {} numbers!", number)
        }
    } else {
        "Must be logged in!".to_string()
    }
}

#[post("/login", data = "<login_form>")]
fn login(login_form: Form<UserLogin>, session: Session) -> Redirect {
    if login_form.username == "admin" && login_form.password == "admin" {
        session.tap(move |sess| {
            sess.push(login_form.username.to_string());
        });
    }
    Redirect::found("/")
}

#[get("/winner")]
pub fn winner<'b>(conn: DbConn, session: Session) -> ContRes<'b> {
    let mut context = create_context("win");
    let mut session_user = String::new();
    session.tap(|sess| {
        for user in sess.iter().take(1) {
            session_user = user.to_owned();
        }
    });
    context.insert("login", &session_user);

    let claims = winner_claims(&conn);

    context.insert("claims", &claims);
    respond_page("winner", context)
}

#[post("/banko", data = "<login_form>")]
fn banko(login_form: Form<Banko>, _session: Session, req: RemoteAddr, conn: DbConn) -> Redirect {
    let remote_ip = req.addr();
    dotenv().ok();
    let allowed_ip = dotenv!("ALLOWED_IP").to_string();
    dbg!(&allowed_ip, &remote_ip);
    if remote_ip == allowed_ip {
        let name: String = login_form.name.to_string();
        let how: i32 = login_form.how;
        let _res = banko_notify(&conn, name, how);
        Redirect::found("/winner")
    } else {
        Redirect::found("/error")
    }
}

#[get("/about")]
pub fn about<'b>(_conn: DbConn, session: Session) -> ContRes<'b> {
    let mut context = create_context("about");
    let mut session_user = String::new();
    session.tap(|sess| {
        for user in sess.iter().take(1) {
            session_user = user.to_owned();
        }
    });
    context.insert("login", &session_user);
    respond_page("about", context)
}

fn main() {
    use crate::errors::*;
    rocket::ignite()
        .attach(DbConn::fairing())
        .attach(Session::fairing())
        .mount(
            "/",
            routes![
                about,
                add_number,
                banko,
                draw,
                error,
                login,
                winner,
                crate::statics::robots_handler,
                crate::statics::favicon_handler,
                crate::statics::static_handler,
            ],
        )
        .register(catchers![page_not_found, bad_request, server_error])
        .launch();
}
