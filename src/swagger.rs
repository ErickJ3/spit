use std::collections::HashMap;

use serde_json::Value;

use crate::MockServerError;

#[derive(Debug)]
pub struct SwaggerState {
    pub components: HashMap<String, Value>,
}

impl SwaggerState {
    pub fn resolve_ref(&self, ref_path: &str) -> Option<Value> {
        let schema_name = ref_path.replace("#/components/schemas/", "");
        self.components.get(&schema_name).cloned()
    }
}

pub async fn parse_swagger(url: &str) -> Result<SwaggerState, MockServerError> {
    let swagger: Value = if url.starts_with("http") {
        reqwest::get(url).await?.json().await?
    } else {
        serde_json::from_str(&std::fs::read_to_string(url)?)?
    };

    let components = swagger
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|schemas| schemas.as_object())
        .map(|schemas| {
            schemas
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();

    Ok(SwaggerState { components })
}

pub fn process_swagger_paths(swagger: &Value) -> HashMap<String, Vec<(String, Value)>> {
    let mut routes = HashMap::new();

    if let Some(paths) = swagger.get("paths").and_then(Value::as_object) {
        for (path, methods) in paths {
            if let Some(method_map) = methods.as_object() {
                let path_handlers = method_map
                    .iter()
                    .map(|(method, definition)| (method.to_uppercase(), definition.clone()))
                    .collect();
                routes.insert(path.clone(), path_handlers);
            }
        }
    }

    routes
}
