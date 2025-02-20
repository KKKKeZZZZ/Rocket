use rocket::{Rocket, Build, futures};
use rocket::fairing::{self, AdHoc};
use rocket::response::status::Created;
use rocket::serde::{Serialize, Deserialize, json::Json};

use rocket_db_pools::{sqlx, Database, Connection};

use futures::{stream::TryStreamExt, future::TryFutureExt};

#[derive(Database)]
#[database("sqlx")]
struct Db(sqlx::SqlitePool);

type Result<T, E = rocket::response::Debug<sqlx::Error>> = std::result::Result<T, E>;

#[derive(Debug, Clone, Deserialize, Serialize, sqlx::FromRow)]
#[serde(crate = "rocket::serde")]
struct Post {// struct edited, viewedTimes added
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    title: String,
    text: String,

    //#[serde(skip_deserializing)]
    viewedTimes: i64,
}

#[post("/post", data = "<post>")]//use post to raise a new post
async fn create(mut db: Connection<Db>, post: Json<Post>) -> Result<Created<Json<Post>>> {
    // There is no support for `RETURNING`.
    sqlx::query!("INSERT INTO posts (title, text, viewedTimes) VALUES (?, ?, ?)", post.title, post.text, post.viewedTimes)
        .execute(&mut *db)
        .await?;

    Ok(Created::new("/").body(post))
}

#[get("/get")]// use get without id to show all
async fn list(mut db: Connection<Db>) -> Result<Json<Vec<i64>>> {
    let ids = sqlx::query!("SELECT id FROM posts")
        .fetch(&mut *db)
        .map_ok(|record| record.id)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(Json(ids))
}

#[get("/get/<id>")]// use get with id to show the data with entered id
async fn read(mut db: Connection<Db>, id: i64) -> Option<Json<Post>> {
    sqlx::query!("SELECT id, title, text, viewedTimes FROM posts WHERE id = ?", id)//viewed times added
        .fetch_one(&mut *db)
        .map_ok(|r| Json(Post { id: Some(r.id), title: r.title, text: r.text, viewedTimes: r.viewedTimes }))
        .await
        .ok()
}

#[delete("/delete/<id>")]// use delete to delete the data with entered id
async fn delete(mut db: Connection<Db>, id: i64) -> Result<Option<()>> {
    let result = sqlx::query!("DELETE FROM posts WHERE id = ?", id)
        .execute(&mut *db)
        .await?;

    Ok((result.rows_affected() == 1).then(|| ()))
}

#[delete("/destroy")]// use destroy to empty the table
async fn destroy(mut db: Connection<Db>) -> Result<()> {
    sqlx::query!("DELETE FROM posts").execute(&mut *db).await?;

    Ok(())
}

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match Db::fetch(&rocket) {
        Some(db) => match sqlx::migrate!("db/sqlx/migrations").run(&**db).await {
            Ok(_) => Ok(rocket),
            Err(e) => {
                error!("Failed to initialize SQLx database: {}", e);
                Err(rocket)
            }
        }
        None => Err(rocket),
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("SQLx Stage", |rocket| async {
        rocket.attach(Db::init())
            .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
            .mount("/sqlx", routes![list, create, read, delete, destroy])
    })
}
