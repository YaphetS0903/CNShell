use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const BROKER_PROTOCOL_VERSION: u32 = 1;
pub const MAX_BROKER_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerRequest {
    pub protocol_version: u32,
    pub generation: String,
    pub broker_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub client_name: String,
    pub executable_path: Option<String>,
    pub executable_sha256: Option<String>,
    pub request_id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerResponse {
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BrokerError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerError {
    pub code: String,
    pub message: String,
}

impl BrokerResponse {
    pub fn success(request_id: impl Into<String>, result: Value) -> Self {
        Self {
            request_id: request_id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            ok: false,
            result: None,
            error: Some(BrokerError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryDocument {
    pub schema_version: u32,
    pub address: String,
    pub generation: String,
    pub broker_token: String,
    pub process_id: u32,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> Value {
        json!({
            "protocolVersion": 1,
            "generation": "generation",
            "brokerToken": "token",
            "clientId": "client",
            "clientSecret": "secret",
            "clientName": "Codex",
            "executablePath": "/tmp/cnshell-mcp",
            "executableSha256": "sha256:digest",
            "requestId": "request",
            "tool": "cnshell_list_connections",
            "arguments": {}
        })
    }

    #[test]
    fn broker_request_rejects_unknown_fields() {
        let mut value = request();
        value["unexpected"] = Value::Bool(true);
        assert!(serde_json::from_value::<BrokerRequest>(value).is_err());
    }

    #[test]
    fn broker_request_defaults_missing_arguments() {
        let mut value = request();
        value.as_object_mut().unwrap().remove("arguments");
        let decoded = serde_json::from_value::<BrokerRequest>(value).unwrap();
        assert!(decoded.arguments.is_empty());
    }

    #[test]
    fn broker_response_has_one_result_channel() {
        let success = BrokerResponse::success("request", json!({"ok": true}));
        assert!(success.ok && success.result.is_some() && success.error.is_none());
        let failure = BrokerResponse::error("request", "denied", "not allowed");
        assert!(!failure.ok && failure.result.is_none() && failure.error.is_some());
    }
}
