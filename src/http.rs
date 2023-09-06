use std::net::SocketAddr;
use std::{path::PathBuf, sync::Arc};

use axum::extract::Path;
use axum::extract::RawQuery;
use axum::response::IntoResponse;
use axum::{
    body::Body as AxumBody,
    extract::State,
    http::{header::HeaderMap, Request},
    response::{
        sse::{Event, KeepAlive},
        Response as AxumResponse, Sse,
    },
    routing::get,
    Router,
};
use futures_util::Stream;
use leptos::*;
use leptos_axum::handle_server_fns_with_context;
use tokio::sync::broadcast::Sender;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::app::App;
use crate::{Args, QueueItem};

#[derive(Debug, Clone)]
struct MailboxesPath(PathBuf);

#[derive(Debug, Clone)]
struct FilesPath(PathBuf);

#[derive(axum::extract::FromRef, Clone)]
pub struct Context {
    path: MailboxesPath,
    sender: Sender<Arc<QueueItem>>,
    leptos_options: LeptosOptions,
}

async fn leptos_routes_handler(
    State(context): State<Context>,
    req: Request<AxumBody>,
) -> AxumResponse {
    let handler = leptos_axum::render_app_to_stream_with_context(
        context.leptos_options.clone(),
        move || {
            provide_context(context.path.0.clone());
        },
        || view! { <App/> },
    );
    handler(req).await.into_response()
}

async fn server_fn_handler(
    State(context): State<Context>,
    path: Path<String>,
    headers: HeaderMap,
    raw_query: RawQuery,
    request: Request<AxumBody>,
) -> impl IntoResponse {
    handle_server_fns_with_context(
        path,
        headers,
        raw_query,
        move || {
            provide_context(context.path.0.clone());
        },
        request,
    )
    .await
}

#[cfg(feature = "bundle")]
static STATIC_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_BUNDLE_DIR");

#[cfg(feature = "bundle")]
async fn static_path(Path(path): Path<String>) -> impl IntoResponse {
    use axum::body;
    use axum::body::Empty;
    use axum::body::Full;
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::Response;

    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_text_plain();

    match STATIC_DIR.get_file(path) {
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(body::boxed(Empty::new()))
            .unwrap(),
        Some(file) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref()).unwrap(),
            )
            .body(body::boxed(Full::from(file.contents())))
            .unwrap(),
    }
}

pub async fn listen(
    args: &Args,
    sender: Sender<Arc<QueueItem>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use leptos_axum::{generate_route_list, LeptosRoutes};

    let conf = get_configuration(None).await.unwrap();
    let routes = generate_route_list(|| view! { <App/> });

    let app = Router::new().route("/sse", get(sse_handler));

    let app = {
        #[cfg(feature = "bundle")]
        {
            app.route("/pkg/*rest", get(static_path))
        }
        #[cfg(not(feature = "bundle"))]
        {
            app.nest_service("/pkg", tower_http::services::fs::ServeDir::new(&args.files))
        }
    };

    let app = app
        .nest_service(
            "/data",
            tower_http::services::fs::ServeDir::new(&args.mailboxes),
        )
        .route(
            "/api/*fn_name",
            get(server_fn_handler).post(server_fn_handler),
        )
        .leptos_routes_with_handler(routes, get(leptos_routes_handler))
        .with_state(Context {
            path: MailboxesPath(args.mailboxes.clone()),
            sender,
            leptos_options: conf.leptos_options,
        });

    let addr: SocketAddr = args.listen_http.parse()?;
    println!("http server listining on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn sse_handler(
    State(context): State<Context>,
) -> Sse<impl Stream<Item = Result<Event, &'static str>>> {
    let receiver = BroadcastStream::new(context.sender.subscribe());

    Sse::new(receiver.map(|mailbox| {
        Event::default()
            .json_data(mailbox.map_err(|_| "failed queue")?)
            .map_err(|_| "failed json")
    }))
    .keep_alive(KeepAlive::default())
}
