use std::collections::HashMap;

use fake::Fake;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

#[derive(Debug, Clone, Serialize)]
pub struct RequestLog {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub response_status: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MockPattern {
    #[serde(rename = "enum")]
    Enum { values: Vec<String> },
    #[serde(rename = "number")]
    Number {
        min: Option<f64>,
        max: Option<f64>,
        #[serde(default)]
        decimals: Option<u32>,
    },
    #[serde(rename = "card")]
    CreditCard {
        #[serde(default = "default_card_length")]
        length: usize,
    },
    #[serde(rename = "date")]
    DateTime { format: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockConfig {
    pub delay: Option<u64>,
    pub status_code: Option<u16>,
    pub headers: Option<HashMap<String, String>>,
    pub fields: Option<MockFieldConfig>,
}

#[derive(Default, Clone, Debug)]
pub struct MockState {
    pub routes: HashMap<String, Vec<(String, Value)>>,
    pub config: MockConfig,
    pub request_log: Vec<RequestLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockFieldConfig {
    pub patterns: HashMap<String, MockPattern>,
}

fn default_card_length() -> usize {
    16
}

impl MockPattern {
    pub fn generate_value(&self) -> serde_json::Value {
        match self {
            MockPattern::Enum { values } => {
                let index = (0..values.len()).fake::<usize>();
                serde_json::Value::String(values[index].clone())
            }
            MockPattern::Number { min, max, decimals } => {
                let min_val = min.unwrap_or(0.0);
                let max_val = max.unwrap_or(100.0);
                let num = min_val + (max_val - min_val) * rand::random::<f64>();

                if let Some(dec) = decimals {
                    let factor = 10_f64.powi(*dec as i32);
                    let rounded = (num * factor).round() / factor;
                    serde_json::Value::Number(Number::from_f64(rounded).unwrap_or(Number::from(0)))
                } else {
                    serde_json::Value::Number(
                        Number::from_f64(num.round()).unwrap_or(Number::from(0)),
                    )
                }
            }
            MockPattern::CreditCard { length } => {
                let card_num: String = (0..*length)
                    .map(|_| rand::random::<u8>() % 10)
                    .map(|n| n.to_string())
                    .collect();
                serde_json::Value::String(card_num)
            }
            MockPattern::DateTime { format } => {
                let now = chrono::Utc::now();
                let formatted = match format {
                    Some(fmt) => now.format(fmt),
                    None => now.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                };
                serde_json::Value::String(formatted.to_string())
            }
        }
    }
}
