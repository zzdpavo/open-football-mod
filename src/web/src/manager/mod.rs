pub mod get;
pub mod tactics;
pub mod transfers;
pub mod r#match;

use axum::Router;
use axum::routing::get;

pub fn manager_routes() -> Router<crate::GameAppData> {
    Router::new()
        .route("/{lang}/manager", get(get::manager_overview))
        .route("/{lang}/manager/tactics", get(tactics::manager_tactics))
        .route("/{lang}/manager/transfers", get(transfers::manager_transfers))
        .route("/{lang}/manager/match", get(r#match::manager_match))
}
