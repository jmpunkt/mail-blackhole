//!

pub mod api;
pub mod app;
#[cfg(feature = "ssr")]
pub mod http;
#[cfg(feature = "ssr")]
pub mod mail;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueueItem {
    obj: MailboxItem,
    receiver: String,
}

#[cfg(feature = "ssr")]
fn files_dir() -> std::path::PathBuf {
    let root: Option<std::path::PathBuf> = match std::env::var("LEPTOS_SITE_ROOT") {
        Ok(val) => Some(
            val.try_into()
                .expect("LEPTOS_SITE_ROOT is not a valid path"),
        ),
        Err(_) => None,
    };

    let dir: Option<std::path::PathBuf> = match std::env::var("LEPTOS_SITE_PKG_DIR") {
        Ok(val) => Some(
            val.try_into()
                .expect("LEPTOS_SITE_PKG_DIR is not a valid path"),
        ),
        Err(_) => None,
    };

    match (root, dir) {
        (Some(root), Some(dir)) => root.join(dir),
        (None, Some(_)) => {
            panic!("environment variable LEPTOS_SITE_ROOT is not set and required without argument `files`")
        }
        (Some(_), None) => {
            panic!("environment variable LEPTOS_SITE_PKG_DIR is not set and required without argument `files`")
        }
        (None, None) => {
            panic!("environment variables LEPTOS_SITE_ROOT and LEPTOS_SITE_PKG_DIR are not set and required without argument `files`")
        }
    }
}

#[cfg(feature = "ssr")]
fn http_addr() -> String {
    match std::env::var("LEPTOS_SITE_ADDR") {
        Ok(val) => val,
        Err(_) => String::from("0.0.0.0:8080"),
    }
}

#[cfg(feature = "ssr")]
#[derive(Debug, argh::FromArgs)]
/// Save all mail
pub struct Args {
    /// listener address for the server (default: 0.0.0.0:2525)
    #[argh(option, default = "String::from(\"0.0.0.0:2525\")")]
    listen_mail: String,

    /// listener address for the server (default: $LEPTOS_SITE_ADDR or 0.0.0.0:8080)
    #[argh(option, default = "http_addr()")]
    listen_http: String,

    /// target directory for mailboxes (default: ./mailboxes)
    #[argh(option, default = "std::path::PathBuf::from(\"./mailboxes\")")]
    mailboxes: std::path::PathBuf,

    #[cfg(not(feature = "bundle"))]
    /// path to directory containing web files (default: $LEPTOS_SITE_PKG_DIR)
    #[argh(option, default = "files_dir()")]
    files: std::path::PathBuf,
}

use api::MailboxItem;
use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "hydrate")] {
        use wasm_bindgen::prelude::wasm_bindgen;
        use app::*;
        use leptos::*;

        #[wasm_bindgen]
        pub fn hydrate() {
            console_error_panic_hook::set_once();
            _ = console_log::init_with_level(log::Level::Debug);

            leptos::mount_to_body(|| {
                view! { <App/> }
            });
        }
    }
}
