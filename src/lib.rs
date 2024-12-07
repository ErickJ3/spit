use std::sync::Mutex;

use actix_web::{middleware::Logger, web, App, HttpServer};
use config::{MockConfig, MockState};
use log::{error, info};
use request::handle_request;
use reqwest;
use serde_json::Value;
use swagger::{process_swagger_paths, SwaggerState};
use thiserror::Error;

pub mod cli;
pub mod config;
pub mod request;
pub mod swagger;

#[derive(Error, Debug)]
pub enum MockServerError {
    #[error("Failed to fetch Swagger: {0}")]
    SwaggerFetch(#[from] reqwest::Error),
    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub fn load_config(
    config_path: &Option<std::path::PathBuf>,
) -> Result<MockConfig, Box<dyn std::error::Error>> {
    if let Some(path) = config_path {
        let content = std::fs::read_to_string(path)?;
        if path
            .extension()
            .map_or(false, |ext| ext == "yaml" || ext == "yml")
        {
            Ok(serde_yaml::from_str(&content)?)
        } else {
            Ok(serde_json::from_str(&content)?)
        }
    } else {
        Ok(MockConfig::default())
    }
}

pub async fn start_server(
    source: &str,
    host: &str,
    port: u16,
    delay: Option<u64>,
    mut config: MockConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));

    info!("Initializing mock server...");
    let swagger = fetch_swagger(source).await?;
    info!("Loaded swagger configuration");

    let swagger_state = web::Data::new(SwaggerState {
        components: swagger
            .get("components")
            .and_then(|c| c.get("schemas"))
            .and_then(|schemas| schemas.as_object())
            .map(|schemas| {
                schemas
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default(),
    });

    if config.delay.is_none() {
        config.delay = delay;
    }

    let routes = process_swagger_paths(&swagger);
    info!("Processed {} routes", routes.len());
    for (path, methods) in &routes {
        info!(
            "Route: {} - Methods: {:?}",
            path,
            methods.iter().map(|(m, _)| m).collect::<Vec<_>>()
        );
    }

    let state = web::Data::new(Mutex::new(MockState {
        routes,
        config,
        request_log: Vec::new(),
    }));

    info!("Starting mock server on http://{}:{}", host, port);

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .app_data(swagger_state.clone())
            .service(web::resource("/{tail:.*}").route(web::route().to(handle_request)))
            .default_service(web::route().to(|req: actix_web::HttpRequest| {
                error!("Unhandled request: {} {}", req.method(), req.path());
                async move {
                    actix_web::HttpResponse::NotFound().json(serde_json::json!({
                        "error": "Route not found",
                        "path": req.path(),
                        "method": req.method().as_str()
                    }))
                }
            }))
    })
    .bind(format!("{}:{}", host, port))?
    .run()
    .await?;

    Ok(())
}

fn validate_path_params(path: &str, req_path: &str) -> bool {
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let req_segments: Vec<&str> = req_path.split('/').filter(|s| !s.is_empty()).collect();

    if path_segments.len() != req_segments.len() {
        return false;
    }

    path_segments
        .iter()
        .zip(req_segments.iter())
        .all(|(path_seg, req_seg)| path_seg.starts_with('{') || path_seg == req_seg)
}

pub async fn fetch_swagger(url: &str) -> Result<Value, MockServerError> {
    if url.starts_with("http") {
        Ok(reqwest::get(url).await?.json().await?)
    } else {
        Ok(serde_json::from_str(&std::fs::read_to_string(url)?)?)
    }
}
