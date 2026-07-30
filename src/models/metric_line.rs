use super::{MetricKind, MetricValue, ModelUsageBreakdown};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressFormat {
    Percent,
    Dollars,
    Count { suffix: String },
}

impl ProgressFormat {
    pub fn metric_kind(&self) -> MetricKind {
        match self {
            ProgressFormat::Percent => MetricKind::Percent,
            ProgressFormat::Dollars => MetricKind::Dollars,
            ProgressFormat::Count { .. } => MetricKind::Count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricChartPoint {
    pub value: f64,
    pub label: String,
    pub value_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetricLine {
    Text {
        label: String,
        value: String,
        color_hex: Option<String>,
        subtitle: Option<String>,
    },
    Values {
        label: String,
        values: Vec<MetricValue>,
        color_hex: Option<String>,
        expiries_at: Vec<i64>,
        unknown_models: Vec<String>,
        model_breakdown: Option<ModelUsageBreakdown>,
    },
    Progress {
        label: String,
        used: f64,
        limit: f64,
        format: ProgressFormat,
        resets_at: Option<i64>,
        period_duration_ms: Option<i64>,
        color_hex: Option<String>,
    },
    Badge {
        label: String,
        text: String,
        color_hex: Option<String>,
        subtitle: Option<String>,
    },
    Chart {
        label: String,
        points: Vec<MetricChartPoint>,
        note: Option<String>,
    },
}

impl MetricLine {
    pub fn label(&self) -> &str {
        match self {
            MetricLine::Text { label, .. }
            | MetricLine::Values { label, .. }
            | MetricLine::Progress { label, .. }
            | MetricLine::Badge { label, .. }
            | MetricLine::Chart { label, .. } => label,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, MetricLine::Badge { label, .. } if label == "Error")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LineType {
    Text,
    Values,
    Progress,
    Badge,
    Chart,
}

// Custom serialization matching Swift's tagged enum encoding
impl Serialize for MetricLine {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("MetricLine", 8)?;
        match self {
            MetricLine::Text {
                label,
                value,
                color_hex,
                subtitle,
            } => {
                state.serialize_field("type", &LineType::Text)?;
                state.serialize_field("label", label)?;
                state.serialize_field("value", value)?;
                state.serialize_field("color_hex", color_hex)?;
                state.serialize_field("subtitle", subtitle)?;
            }
            MetricLine::Values {
                label,
                values,
                color_hex,
                expiries_at,
                unknown_models,
                model_breakdown,
            } => {
                state.serialize_field("type", &LineType::Values)?;
                state.serialize_field("label", label)?;
                state.serialize_field("values", values)?;
                state.serialize_field("color_hex", color_hex)?;
                state.serialize_field("expiries_at", expiries_at)?;
                state.serialize_field("unknown_models", unknown_models)?;
                state.serialize_field("model_breakdown", model_breakdown)?;
            }
            MetricLine::Progress {
                label,
                used,
                limit,
                format,
                resets_at,
                period_duration_ms,
                color_hex,
            } => {
                state.serialize_field("type", &LineType::Progress)?;
                state.serialize_field("label", label)?;
                state.serialize_field("used", used)?;
                state.serialize_field("limit", limit)?;
                state.serialize_field("format", format)?;
                state.serialize_field("resets_at", resets_at)?;
                state.serialize_field("period_duration_ms", period_duration_ms)?;
                state.serialize_field("color_hex", color_hex)?;
            }
            MetricLine::Badge {
                label,
                text,
                color_hex,
                subtitle,
            } => {
                state.serialize_field("type", &LineType::Badge)?;
                state.serialize_field("label", label)?;
                state.serialize_field("text", text)?;
                state.serialize_field("color_hex", color_hex)?;
                state.serialize_field("subtitle", subtitle)?;
            }
            MetricLine::Chart {
                label,
                points,
                note,
            } => {
                state.serialize_field("type", &LineType::Chart)?;
                state.serialize_field("label", label)?;
                state.serialize_field("points", points)?;
                state.serialize_field("note", note)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for MetricLine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Type,
            Label,
            Value,
            Values,
            Used,
            Limit,
            Format,
            ResetsAt,
            ExpiriesAt,
            UnknownModels,
            ModelBreakdown,
            PeriodDurationMs,
            ColorHex,
            Subtitle,
            Text,
            Points,
            Note,
        }

        struct MetricLineVisitor;
        impl<'de> Visitor<'de> for MetricLineVisitor {
            type Value = MetricLine;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct MetricLine")
            }
            fn visit_map<V: MapAccess<'de>>(self, mut map: V) -> Result<MetricLine, V::Error> {
                let mut line_type: Option<LineType> = None;
                let mut label: Option<String> = None;
                let mut value: Option<String> = None;
                let mut values: Option<Vec<MetricValue>> = None;
                let mut used: Option<f64> = None;
                let mut limit: Option<f64> = None;
                let mut format: Option<ProgressFormat> = None;
                let mut resets_at: Option<Option<i64>> = None;
                let mut expiries_at: Option<Vec<i64>> = None;
                let mut unknown_models: Option<Vec<String>> = None;
                let mut model_breakdown: Option<Option<ModelUsageBreakdown>> = None;
                let mut period_duration_ms: Option<Option<i64>> = None;
                let mut color_hex: Option<Option<String>> = None;
                let mut subtitle: Option<Option<String>> = None;
                let mut text: Option<String> = None;
                let mut points: Option<Vec<MetricChartPoint>> = None;
                let mut note: Option<Option<String>> = None;

                while let Some(key) = map.next_key::<Field>()? {
                    match key {
                        Field::Type => {
                            line_type = Some(map.next_value()?);
                        }
                        Field::Label => {
                            label = Some(map.next_value()?);
                        }
                        Field::Value => {
                            value = Some(map.next_value()?);
                        }
                        Field::Values => {
                            values = Some(map.next_value()?);
                        }
                        Field::Used => {
                            used = Some(map.next_value()?);
                        }
                        Field::Limit => {
                            limit = Some(map.next_value()?);
                        }
                        Field::Format => {
                            format = Some(map.next_value()?);
                        }
                        Field::ResetsAt => {
                            resets_at = Some(map.next_value()?);
                        }
                        Field::ExpiriesAt => {
                            expiries_at = Some(map.next_value()?);
                        }
                        Field::UnknownModels => {
                            unknown_models = Some(map.next_value()?);
                        }
                        Field::ModelBreakdown => {
                            model_breakdown = Some(map.next_value()?);
                        }
                        Field::PeriodDurationMs => {
                            period_duration_ms = Some(map.next_value()?);
                        }
                        Field::ColorHex => {
                            color_hex = Some(map.next_value()?);
                        }
                        Field::Subtitle => {
                            subtitle = Some(map.next_value()?);
                        }
                        Field::Text => {
                            text = Some(map.next_value()?);
                        }
                        Field::Points => {
                            points = Some(map.next_value()?);
                        }
                        Field::Note => {
                            note = Some(map.next_value()?);
                        }
                    }
                }
                let label = label.ok_or_else(|| de::Error::missing_field("label"))?;
                Ok(
                    match line_type.ok_or_else(|| de::Error::missing_field("type"))? {
                        LineType::Text => MetricLine::Text {
                            label,
                            value: value.ok_or_else(|| de::Error::missing_field("value"))?,
                            color_hex: color_hex.unwrap_or(None),
                            subtitle: subtitle.unwrap_or(None),
                        },
                        LineType::Values => MetricLine::Values {
                            label,
                            values: values.ok_or_else(|| de::Error::missing_field("values"))?,
                            color_hex: color_hex.unwrap_or(None),
                            expiries_at: expiries_at.unwrap_or_default(),
                            unknown_models: unknown_models.unwrap_or_default(),
                            model_breakdown: model_breakdown.unwrap_or(None),
                        },
                        LineType::Progress => MetricLine::Progress {
                            label,
                            used: used.ok_or_else(|| de::Error::missing_field("used"))?,
                            limit: limit.ok_or_else(|| de::Error::missing_field("limit"))?,
                            format: format.ok_or_else(|| de::Error::missing_field("format"))?,
                            resets_at: resets_at.unwrap_or(None),
                            period_duration_ms: period_duration_ms.unwrap_or(None),
                            color_hex: color_hex.unwrap_or(None),
                        },
                        LineType::Badge => MetricLine::Badge {
                            label,
                            text: text.ok_or_else(|| de::Error::missing_field("text"))?,
                            color_hex: color_hex.unwrap_or(None),
                            subtitle: subtitle.unwrap_or(None),
                        },
                        LineType::Chart => MetricLine::Chart {
                            label,
                            points: points.ok_or_else(|| de::Error::missing_field("points"))?,
                            note: note.unwrap_or(None),
                        },
                    },
                )
            }
        }
        deserializer.deserialize_struct(
            "MetricLine",
            &[
                "type",
                "label",
                "value",
                "values",
                "used",
                "limit",
                "format",
                "resets_at",
                "expiries_at",
                "unknown_models",
                "model_breakdown",
                "period_duration_ms",
                "color_hex",
                "subtitle",
                "text",
                "points",
                "note",
            ],
            MetricLineVisitor,
        )
    }
}
