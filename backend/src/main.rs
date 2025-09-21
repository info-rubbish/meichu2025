mod config;
mod errors;
mod middlewares;
mod openrouter;
mod prompts;
mod routes;
mod sse;
mod tools;
mod utils;

use std::sync::Arc;

use crate::{openrouter::Openrouter, prompts::PromptEnv, tools::ToolStore};
use anyhow::Context;
use axum::{Router, middleware};
use betrayer::{
    Icon, Menu, MenuItem, TrayEvent, TrayIcon, TrayIconBuilder, winit::WinitTrayIconBuilderExt,
};
use dotenv::var;
use entity::prelude::*;
use middlewares::cache_control::CacheControlLayer;
use migration::MigratorTrait;
use pasetors::{keys::SymmetricKey, version4::V4};
use sea_orm::{Database, DbConn, EntityTrait};
use sse::SseContext;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tracing::Level;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt};
use utils::password_hash::Hasher;
use winit::{
    application::ApplicationHandler,
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[cfg(feature = "dev")]
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

pub struct AppState {
    pub conn: DbConn,
    pub key: SymmetricKey<V4>,
    pub sse: SseContext,
    pub prompt: PromptEnv,
    pub hasher: Hasher,
    pub openrouter: Openrouter,
    pub tools: ToolStore,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter::Targets::new().with_target("backend", Level::TRACE))
        .init();

    let database_url = var("DATABASE_URL").unwrap_or("sqlite://db.sqlite?mode=rwc".to_owned());
    let bind_addr = var("BIND_ADDR").unwrap_or("0.0.0.0:8001".to_owned());
    let static_dir = var("STATIC_DIR").unwrap_or("../frontend/build".to_owned());

    migration::migrate(&database_url)
        .await
        .expect("Migration failed");

    let conn = Database::connect(database_url)
        .await
        .expect("Cannot connect to database");

    migration::Migrator::up(&conn, None)
        .await
        .expect("Cannot migrate database");

    let key = SymmetricKey::from(
        &Config::find_by_id("paseto_key")
            .one(&conn)
            .await
            .unwrap()
            .context("Cannot find paseto key")
            .unwrap()
            .value,
    )
    .expect("Cannot parse paseto key");

    let sse = SseContext::new(conn.clone());
    let prompt = PromptEnv::new(conn.clone());
    let openrouter = Openrouter::new();
    let mut tools = ToolStore::new(conn.clone());

    tools.add_tool::<tools::wttr::Wttr>().unwrap();
    tools.add_tool::<tools::nearbyplace::NearByPlace>().unwrap();
    tools.add_tool::<tools::mail::RecentMail>().unwrap();
    tools.add_tool::<tools::mail::ReplyMail>().unwrap();
    tools.add_tool::<tools::mail::SendMail>().unwrap();
    tools.add_tool::<tools::mail::GetMailContent>().unwrap();
    tools.add_tool::<tools::rss::RssSearch>().unwrap();

    let state = Arc::new(AppState {
        conn,
        key,
        sse,
        hasher: Hasher::default(),
        openrouter,
        prompt,
        tools,
    });

    let var_name = Router::new();
    let app = var_name
        .nest(
            "/api",
            Router::new()
                .nest("/chat", routes::chat::routes())
                .nest("/user", routes::user::routes())
                .nest("/message", routes::message::routes())
                .nest("/model", routes::model::routes())
                .layer(middleware::from_extractor_with_state::<
                    middlewares::auth::Middleware,
                    _,
                >(state.clone()))
                .nest("/auth", routes::auth::routes()),
        )
        .fallback_service(
            ServiceBuilder::new().layer(CacheControlLayer).service(
                ServeDir::new(static_dir.to_owned())
                    .precompressed_gzip()
                    .precompressed_br()
                    .fallback(
                        ServeFile::new(format!("{}/index.html", static_dir))
                            .precompressed_br()
                            .precompressed_gzip(),
                    ),
            ),
        )
        .with_state(state);

    #[cfg(feature = "dev")]
    let app = app.layer(
        CorsLayer::new()
            .allow_methods(AllowMethods::any())
            .allow_origin(AllowOrigin::any())
            .allow_headers(AllowHeaders::list([
                http::header::AUTHORIZATION,
                http::header::CONTENT_TYPE,
            ])),
    );

    let tcp = TcpListener::bind(bind_addr).await.unwrap();
    tokio::spawn(async {
        axum::serve(tcp, app).await.unwrap();
    })
    .await;
    // tray().unwrap();
}

// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
// enum Signal {
//     Profile(u32),
//     Open,
//     Quit,
// }

// fn tray() -> anyhow::Result<()> {
//     let event_loop = EventLoop::with_user_event().build()?;

//     let tray = TrayIconBuilder::new()
//         .with_icon(Icon::from_png_bytes(include_bytes!(concat!(
//             env!("CARGO_MANIFEST_DIR"),
//             "/../frontend/static/favicon-96x96.png"
//         )))?)
//         .with_tooltip("Demo System Tray")
//         .with_menu(build_menu())
//         .build_event_loop(&event_loop, |e| Some(e))?;

//     event_loop.set_control_flow(ControlFlow::Wait);
//     event_loop.run_app(&mut App { tray })?;
//     Ok(())
// }

// struct App {
//     tray: TrayIcon<Signal>,
// }

// impl ApplicationHandler<TrayEvent<Signal>> for App {
//     fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}
//     fn user_event(&mut self, event_loop: &ActiveEventLoop, event: TrayEvent<Signal>) {
//         if let TrayEvent::Menu(signal) = event {
//             match signal {
//                 Signal::Profile(i) => {
//                     self.tray.set_tooltip(format!("Active Profile: Hi"));
//                     self.tray.set_menu(build_menu());
//                 }
//                 Signal::Open => {}
//                 Signal::Quit => event_loop.exit(),
//             }
//         }
//     }
//     fn window_event(
//         &mut self,
//         _event_loop: &ActiveEventLoop,
//         _window_id: WindowId,
//         _event: WindowEvent,
//     ) {
//     }
// }

// fn build_menu() -> Menu<Signal> {
//     Menu::new([MenuItem::button("Quit", Signal::Quit)])
// }
