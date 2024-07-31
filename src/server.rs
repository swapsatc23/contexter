use crate::config::Config;
use crate::contexter::{concatenate_files, gather_relevant_files};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub config: Arc<RwLock<Config>>,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectListResponse {
    pub projects: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectContentResponse {
    pub content: String,
}

#[derive(Deserialize)]
pub struct ApiKeyQuery {
    pub api_key: String,
}

pub async fn validate_api_key(config: &Config, api_key: &str) -> bool {
    config.api_keys.contains(&api_key.to_string())
}

pub async fn list_projects(
    data: web::Data<AppState>,
    query: web::Query<ApiKeyQuery>,
) -> impl Responder {
    let config = data.config.read().await;
    if !validate_api_key(&config, &query.api_key).await {
        return HttpResponse::Unauthorized().finish();
    }

    let projects: Vec<String> = config.projects.keys().cloned().collect();
    HttpResponse::Ok().json(ProjectListResponse { projects })
}

pub async fn get_project_content(
    project_name: web::Path<String>,
    data: web::Data<AppState>,
    query: web::Query<ApiKeyQuery>,
) -> impl Responder {
    let config = data.config.read().await;
    if !validate_api_key(&config, &query.api_key).await {
        return HttpResponse::Unauthorized().finish();
    }

    let project_name = project_name.into_inner();

    if let Some(project_path) = config.projects.get(&project_name) {
        debug!("Gathering context for project: {}", project_name);
        match gather_relevant_files(project_path.to_str().unwrap(), vec![], vec![]) {
            Ok(files) => match concatenate_files(files) {
                Ok((content, _)) => HttpResponse::Ok().json(ProjectContentResponse { content }),
                Err(e) => {
                    error!("Error concatenating files: {}", e);
                    HttpResponse::InternalServerError().finish()
                }
            },
            Err(e) => {
                error!("Error gathering files: {}", e);
                HttpResponse::InternalServerError().finish()
            }
        }
    } else {
        HttpResponse::NotFound().body(format!("Project '{}' not found", project_name))
    }
}

pub async fn run_server(config: Config, quiet: bool, verbose: bool) -> std::io::Result<()> {
    // Setup logging
    if verbose {
        std::env::set_var("RUST_LOG", "debug");
    } else if !quiet {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let app_state = web::Data::new(AppState {
        config: Arc::new(RwLock::new(config)),
    });

    // Read configuration values before starting the server
    let listen_address = app_state.config.read().await.listen_address.clone();
    let port = app_state.config.read().await.port;

    info!("Starting server on {}:{}", listen_address, port);

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/projects", web::get().to(list_projects))
            .route("/project/{name}", web::get().to(get_project_content))
    })
    .bind((listen_address, port))?
    .run()
    .await
}
