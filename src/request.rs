use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use fake::Fake;
use log::{debug, error};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use crate::{
    config::{MockConfig, MockFieldConfig, MockState, RequestLog},
    swagger::SwaggerState,
    validate_path_params,
};

pub struct RequestHandler {
    req: HttpRequest,
    path: String,
    state: web::Data<Mutex<MockState>>,
    swagger_state: web::Data<SwaggerState>,
}

impl RequestHandler {
    pub fn new(
        req: HttpRequest,
        path: web::Path<String>,
        state: web::Data<Mutex<MockState>>,
        swagger_state: web::Data<SwaggerState>,
    ) -> Self {
        Self {
            req,
            path: format!("/{}", path.as_str()),
            state,
            swagger_state,
        }
    }

    pub async fn handle_request(&self, body: Option<web::Bytes>) -> HttpResponse {
        debug!("Received request: {} {}", self.req.method(), self.path);

        let mut state_guard = match self.acquire_state_lock() {
            Ok(guard) => guard,
            Err(response) => return response,
        };

        let route_result = self.find_matching_route(&state_guard);

        let response = match route_result {
            Ok((route_path, handlers)) => {
                let config = state_guard.config.clone();

                self.process_route(route_path, handlers, &body, &config)
                    .await
            }
            Err(response) => response,
        };

        self.log_request(&mut state_guard, response.status().as_u16());

        response
    }

    fn acquire_state_lock(&self) -> Result<std::sync::MutexGuard<'_, MockState>, HttpResponse> {
        self.state.lock().map_err(|e| {
            error!("Failed to acquire state lock: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Internal server error",
                "details": "Failed to acquire state lock"
            }))
        })
    }

    fn find_matching_route<'a>(
        &self,
        state: &'a MockState,
    ) -> Result<(&'a String, &'a Vec<(String, Value)>), HttpResponse> {
        let matching_route = state.routes.iter().find(|(route_path, _)| {
            let matches = validate_path_params(route_path, &self.path);
            debug!(
                "Checking route '{}' against '{}': {}",
                route_path, self.path, matches
            );
            matches
        });

        matching_route.ok_or_else(|| {
            error!("No matching route found for {}", self.path);
            HttpResponse::NotFound().json(json!({
                "error": "Route not found",
                "requested_path": self.path,
                "method": self.req.method().as_str()
            }))
        })
    }

    async fn process_route(
        &self,
        route_path: &str,
        handlers: &Vec<(String, Value)>,
        body: &Option<web::Bytes>,
        config: &MockConfig,
    ) -> HttpResponse {
        debug!("Found matching route: {}", route_path);
        let method = self.req.method().as_str();

        match handlers.iter().find(|(m, _)| m == method) {
            Some((_, route_schema)) => self.handle_matched_route(route_schema, body, config).await,
            None => {
                error!(
                    "No handler found for method {} on route {}",
                    method, route_path
                );
                HttpResponse::MethodNotAllowed().json(json!({
                    "error": "Method not allowed",
                    "allowed_methods": handlers.iter()
                        .map(|(m, _)| m.clone())
                        .collect::<Vec<String>>()
                }))
            }
        }
    }

    async fn handle_matched_route(
        &self,
        route_schema: &Value,
        body: &Option<web::Bytes>,
        config: &MockConfig,
    ) -> HttpResponse {
        debug!("Found matching method handler for {}", self.req.method());

        if let Some(parameters) = route_schema.get("parameters") {
            if let Err(error_response) = self.validate_headers(parameters) {
                return error_response;
            }
        }

        if let Err(error_response) = self.validate_request_body(body, route_schema) {
            return error_response;
        }

        if let Some(delay) = config.delay {
            debug!("Applying configured delay of {}ms", delay);
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        self.generate_response(route_schema, config)
    }

    fn validate_headers(&self, parameters: &Value) -> Result<(), HttpResponse> {
        let required_headers: Vec<String> = parameters
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|param| {
                if param.get("in") == Some(&json!("header"))
                    && param.get("required") == Some(&json!(true))
                {
                    param.get("name").and_then(Value::as_str).map(String::from)
                } else {
                    None
                }
            })
            .collect();

        let missing_headers: Vec<String> = required_headers
            .iter()
            .filter(|header| !self.req.headers().contains_key(header.to_lowercase()))
            .cloned()
            .collect();

        if !missing_headers.is_empty() {
            debug!("Missing required headers: {:?}", missing_headers);
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Missing required headers",
                "missing_headers": missing_headers
            })));
        }

        Ok(())
    }

    fn validate_request_body(
        &self,
        body: &Option<web::Bytes>,
        schema: &Value,
    ) -> Result<(), HttpResponse> {
        let request_body = match schema.get("requestBody") {
            Some(body) => body,
            None => return Ok(()),
        };

        let body_schema = match request_body
            .get("content")
            .and_then(|content| content.get("application/json"))
            .and_then(|json| json.get("schema"))
        {
            Some(schema) => schema,
            None => return Ok(()),
        };

        if body.is_none()
            && request_body
                .get("required")
                .and_then(|required| required.as_bool())
                .unwrap_or(false)
        {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Missing required request body"
            })));
        }

        if let Some(body_bytes) = body {
            let body_value = match serde_json::from_slice::<Value>(body_bytes) {
                Ok(value) => value,
                Err(e) => {
                    return Err(HttpResponse::BadRequest().json(json!({
                        "error": "Invalid JSON in request body",
                        "details": e.to_string()
                    })));
                }
            };

            self.validate_against_schema(&body_value, body_schema)?;
        }

        Ok(())
    }

    fn validate_against_schema(&self, value: &Value, schema: &Value) -> Result<(), HttpResponse> {
        if let Some(ref_path) = schema.get("$ref").and_then(Value::as_str) {
            if let Some(resolved_schema) = self.swagger_state.resolve_ref(ref_path) {
                return self.validate_against_schema(value, &resolved_schema);
            }
        }

        match schema.get("type").and_then(Value::as_str) {
            Some("object") => self.validate_object(value, schema),
            Some("array") => self.validate_array(value, schema),
            Some("string") => self.validate_string(value, schema),
            Some("number") | Some("integer") => self.validate_number(value, schema),
            Some("boolean") => self.validate_boolean(value),
            _ => Ok(()),
        }
    }

    fn validate_object(&self, value: &Value, schema: &Value) -> Result<(), HttpResponse> {
        if !value.is_object() {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Expected object type"
            })));
        }

        let obj = value.as_object().unwrap();

        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            let missing_fields: Vec<String> = required
                .iter()
                .filter_map(Value::as_str)
                .filter(|&field| !obj.contains_key(field))
                .map(String::from)
                .collect();

            if !missing_fields.is_empty() {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "Missing required fields",
                    "fields": missing_fields
                })));
            }
        }

        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (prop_name, prop_schema) in properties {
                if let Some(prop_value) = obj.get(prop_name) {
                    self.validate_against_schema(prop_value, prop_schema)?;
                }
            }
        }

        Ok(())
    }

    fn validate_array(&self, value: &Value, schema: &Value) -> Result<(), HttpResponse> {
        if !value.is_array() {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Expected array type"
            })));
        }

        let arr = value.as_array().unwrap();

        if let Some(min_items) = schema.get("minItems").and_then(Value::as_u64) {
            if (arr.len() as u64) < min_items {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "Array too short",
                    "minItems": min_items,
                    "actual": arr.len()
                })));
            }
        }

        if let Some(max_items) = schema.get("maxItems").and_then(Value::as_u64) {
            if (arr.len() as u64) > max_items {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "Array too long",
                    "maxItems": max_items,
                    "actual": arr.len()
                })));
            }
        }

        if let Some(items_schema) = schema.get("items") {
            for item in arr {
                self.validate_against_schema(item, items_schema)?;
            }
        }

        Ok(())
    }

    fn validate_string(&self, value: &Value, schema: &Value) -> Result<(), HttpResponse> {
        if !value.is_string() {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Expected string type"
            })));
        }

        let s = value.as_str().unwrap();

        if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64) {
            if (s.len() as u64) < min_length {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "String too short",
                    "minLength": min_length,
                    "actual": s.len()
                })));
            }
        }

        if let Some(max_length) = schema.get("maxLength").and_then(Value::as_u64) {
            if (s.len() as u64) > max_length {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "String too long",
                    "maxLength": max_length,
                    "actual": s.len()
                })));
            }
        }

        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            let regex = Regex::new(pattern).map_err(|_| {
                HttpResponse::InternalServerError().json(json!({
                    "error": "Invalid pattern in schema"
                }))
            })?;

            if !regex.is_match(s) {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "String does not match pattern",
                    "pattern": pattern
                })));
            }
        }

        Ok(())
    }

    fn validate_number(&self, value: &Value, schema: &Value) -> Result<(), HttpResponse> {
        if !value.is_number() {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Expected numeric type"
            })));
        }

        let num = value.as_f64().unwrap();

        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
            if num < minimum {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "Number too small",
                    "minimum": minimum,
                    "actual": num
                })));
            }
        }

        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
            if num > maximum {
                return Err(HttpResponse::BadRequest().json(json!({
                    "error": "Number too large",
                    "maximum": maximum,
                    "actual": num
                })));
            }
        }

        Ok(())
    }

    fn validate_boolean(&self, value: &Value) -> Result<(), HttpResponse> {
        if !value.is_boolean() {
            return Err(HttpResponse::BadRequest().json(json!({
                "error": "Expected boolean type"
            })));
        }
        Ok(())
    }

    fn generate_response(&self, schema: &Value, config: &MockConfig) -> HttpResponse {
        let status_code = config.status_code.unwrap_or(200).try_into().unwrap_or(200);
        let mut response_builder = HttpResponse::build(
            actix_web::http::StatusCode::from_u16(status_code)
                .unwrap_or(actix_web::http::StatusCode::OK),
        );

        if let Some(headers) = &config.headers {
            for (key, value) in headers {
                response_builder.insert_header((key.clone(), value.clone()));
            }
        }

        let response_schema = schema
            .get("responses")
            .and_then(|responses| responses.get(&status_code.to_string()))
            .and_then(|response| response.get("content"))
            .and_then(|content| content.get("application/json"))
            .and_then(|json_content| json_content.get("schema"));

        if let Some(schema) = response_schema {
            if let Some(ref_path) = schema.get("$ref").and_then(Value::as_str) {
                if let Some(resolved_schema) = self.swagger_state.resolve_ref(ref_path) {
                    return response_builder.json(self.generate_mock_value(
                        &resolved_schema,
                        config.fields.as_ref(),
                        None,
                    ));
                }
            }
            return response_builder.json(self.generate_mock_value(
                schema,
                config.fields.as_ref(),
                None,
            ));
        }

        response_builder.json(json!({
            "success": false,
            "message": "Schema not found",
            "data": null
        }))
    }

    fn generate_mock_value(
        &self,
        schema: &Value,
        field_config: Option<&MockFieldConfig>,
        field_name: Option<&str>,
    ) -> Value {
        if let Some(config) = field_config {
            if let Some(name) = field_name {
                if let Some(pattern) = config.patterns.get(name) {
                    return pattern.generate_value();
                }
            }
        }

        match schema {
            Value::Object(map) => {
                if let Some(ref_path) = map.get("$ref").and_then(Value::as_str) {
                    if let Some(resolved_schema) = self.swagger_state.resolve_ref(ref_path) {
                        return self.generate_mock_value(
                            &resolved_schema,
                            field_config,
                            field_name,
                        );
                    }
                }

                let type_val = map.get("type").and_then(Value::as_str).unwrap_or("object");
                match type_val {
                    "string" => self.generate_mock_string(map),
                    "integer" | "number" => self.generate_mock_number(map, type_val),
                    "boolean" => json!(rand::random::<bool>()),
                    "array" => self.generate_mock_array(map, field_config, field_name),
                    "object" => self.generate_mock_object(map, field_config),
                    _ => json!(null),
                }
            }
            _ => json!(null),
        }
    }

    fn generate_mock_string(&self, schema: &serde_json::Map<String, Value>) -> Value {
        use fake::faker::company::raw::*;
        use fake::faker::internet::raw::*;
        use fake::faker::lorem::raw::*;
        use fake::faker::name::raw::*;
        use fake::locales::EN;
        use fake::Fake;

        if let Some(format) = schema.get("format").and_then(Value::as_str) {
            match format {
                "date-time" => json!(chrono::Utc::now().to_rfc3339()),
                "email" => json!(FreeEmail(EN).fake::<String>()),
                "uuid" => json!(uuid::Uuid::new_v4().to_string()),
                "name" => json!(Name(EN).fake::<String>()),
                "username" => json!(Username(EN).fake::<String>()),
                "company" => json!(CompanyName(EN).fake::<String>()),
                _ => json!(Sentence(EN, 3..10).fake::<String>()),
            }
        } else if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
            if !enum_values.is_empty() {
                let index = (0..enum_values.len()).fake::<usize>();
                enum_values[index].clone()
            } else {
                json!(Sentence(EN, 3..10).fake::<String>())
            }
        } else {
            json!(Sentence(EN, 3..10).fake::<String>())
        }
    }

    fn generate_mock_number(
        &self,
        schema: &serde_json::Map<String, Value>,
        type_val: &str,
    ) -> Value {
        let min = schema
            .get("minimum")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let max = schema
            .get("maximum")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);

        if type_val == "integer" {
            json!((min as i64..=max as i64).fake::<i64>())
        } else {
            json!((min + (max - min) * rand::random::<f64>()).round() / 100.0)
        }
    }

    fn generate_mock_array(
        &self,
        schema: &serde_json::Map<String, Value>,
        field_config: Option<&MockFieldConfig>,
        field_name: Option<&str>,
    ) -> Value {
        let min_items = schema.get("minItems").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let max_items = schema.get("maxItems").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
        let count = (min_items..=max_items).fake::<usize>();

        if let Some(items) = schema.get("items") {
            json!((0..count)
                .map(|_| self.generate_mock_value(items, field_config, field_name))
                .collect::<Vec<_>>())
        } else {
            json!([])
        }
    }

    fn generate_mock_object(
        &self,
        schema: &serde_json::Map<String, Value>,
        field_config: Option<&MockFieldConfig>,
    ) -> Value {
        let mut mock = serde_json::Map::new();

        let props = match schema.get("properties").and_then(Value::as_object) {
            Some(props) => props,
            None => return Value::Object(mock),
        };

        let required_fields: HashSet<_> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|req| req.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        for (key, prop_schema) in props {
            if required_fields.contains(key.as_str()) || required_fields.is_empty() {
                mock.insert(
                    key.clone(),
                    self.generate_mock_value(prop_schema, field_config, Some(key)),
                );
            }
        }

        Value::Object(mock)
    }

    fn log_request(&self, state: &mut MockState, status: u16) {
        let headers: HashMap<String, String> = self
            .req
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();

        state.request_log.push(RequestLog {
            timestamp: Utc::now(),
            method: self.req.method().to_string(),
            path: self.path.clone(),
            headers,
            response_status: status,
        });
    }
}

pub async fn handle_request(
    req: HttpRequest,
    path: web::Path<String>,
    body: Option<web::Bytes>,
    state: web::Data<Mutex<MockState>>,
    swagger_state: web::Data<SwaggerState>,
) -> HttpResponse {
    let handler = RequestHandler::new(req, path, state, swagger_state);
    handler.handle_request(body).await
}
