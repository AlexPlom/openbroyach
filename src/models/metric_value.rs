use super::MetricKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricValue {
    pub number: f64,
    pub kind: MetricKind,
    pub label: Option<String>,
    #[serde(default)]
    pub estimated: bool,
}

impl MetricValue {
    pub fn new(number: f64, kind: MetricKind) -> Self {
        Self {
            number,
            kind,
            label: None,
            estimated: false,
        }
    }

    pub fn with_label(number: f64, kind: MetricKind, label: &str) -> Self {
        Self {
            number,
            kind,
            label: Some(label.to_string()),
            estimated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueSelection {
    All,
    Kind(MetricKind),
}

impl ValueSelection {
    pub fn apply(&self, values: &[MetricValue]) -> Vec<MetricValue> {
        match self {
            ValueSelection::All => values.to_vec(),
            ValueSelection::Kind(kind) => {
                values.iter().filter(|v| v.kind == *kind).cloned().collect()
            }
        }
    }
}
