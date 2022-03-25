use warp::{http, Filter};
use crate::entry::Entry;

pub fn create_routes() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
	get_record().or(put_record())
}

fn get_record() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
	warp::path("get")
		.and(warp::path::end())
	        .and(warp::get())
		.and(json_body())
		.and_then(get_record_fn)
}

fn put_record() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
	warp::path("put")
		.and(warp::path::end())
		.and(warp::post())
		.and(json_body())
		.and_then(put_record_fn)
}

async fn get_record_fn(item: Entry) -> Result<impl warp::Reply, warp::Rejection> {
	println!("{:?}", item);
        Ok(warp::reply::with_status(
            "got",
            http::StatusCode::CREATED,
        ))
}

async fn put_record_fn(item: Entry) -> Result<impl warp::Reply, warp::Rejection> {
	println!("{:?}", item);
        Ok(warp::reply::with_status(
            "created",
            http::StatusCode::CREATED,
        ))
}

fn json_body() -> impl Filter<Extract = (Entry,), Error = warp::Rejection> + Clone {
    // When accepting a body, we want a JSON body
    // (and to reject huge payloads)...
    warp::body::content_length_limit(1024 * 16).and(warp::body::json())
}


