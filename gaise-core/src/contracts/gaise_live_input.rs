use serde_json::Value;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GaiseLiveInput {
    Audio {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        sample_rate: u32,
    },
    Text {
        text: String,
    },
    ToolResponse {
        call_id: String,
        name: String,
        result: Value,
    },
    Close,
}
