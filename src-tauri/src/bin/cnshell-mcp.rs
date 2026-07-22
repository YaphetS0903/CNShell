use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, GetPromptRequestParams,
        GetPromptResult, Implementation, ListPromptsResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, Prompt, PromptArgument, PromptMessage, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, Role, ServerCapabilities, ServerInfo, Tool,
        ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
};
use serde_json::{Map, Value, json};
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncRead, ReadBuf};

const MAX_MCP_MESSAGE_BYTES: usize = 1024 * 1024;

struct LimitedLineReader<R> {
    inner: R,
    current_line_bytes: usize,
}

impl<R> LimitedLineReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            current_line_bytes: 0,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for LimitedLineReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut temporary = vec![0_u8; buffer.remaining().min(8 * 1024)];
        let mut temporary_buffer = ReadBuf::new(&mut temporary);
        match Pin::new(&mut self.inner).poll_read(context, &mut temporary_buffer) {
            Poll::Ready(Ok(())) => {
                let added = temporary_buffer.filled();
                let mut current = self.current_line_bytes;
                for segment in added.split_inclusive(|byte| *byte == b'\n') {
                    current = current.saturating_add(segment.len());
                    if current > MAX_MCP_MESSAGE_BYTES {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "MCP input message exceeds 1 MiB",
                        )));
                    }
                    if segment.last() == Some(&b'\n') {
                        current = 0;
                    }
                }
                self.current_line_bytes = current;
                buffer.put_slice(added);
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[derive(Clone)]
struct CnshellMcpServer {
    client_id: String,
    client_name: String,
    discovery: PathBuf,
}

impl CnshellMcpServer {
    async fn call(
        &self,
        request: CallToolRequestParams,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> CallToolResult {
        let arguments = request.arguments.unwrap_or_default();
        match cnshell_lib::mcp::broker_call(
            &self.client_id,
            &self.client_name,
            &self.discovery,
            &request.name,
            arguments,
            cancellation,
        )
        .await
        {
            Ok(value) => bounded_result(value, false),
            Err(error) => bounded_result(
                json!({
                    "code": "cnshell_error",
                    "message": error.to_string(),
                }),
                true,
            ),
        }
    }
}

impl ServerHandler for CnshellMcpServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Ok(self.call(request, context.ct).await)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tool_definitions()))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(resource_definitions()))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if let Some(result) = static_resource_result(&request.uri) {
            return Ok(result);
        }
        if !dynamic_resource_uri(&request.uri) {
            return Err(McpError::resource_not_found(
                "Unknown CNshell resource",
                None,
            ));
        }
        let value = cnshell_lib::mcp::broker_resource_call(
            &self.client_id,
            &self.client_name,
            &self.discovery,
            &request.uri,
            context.ct,
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        dynamic_resource_result(request.uri, value)
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(prompt_definitions()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        get_prompt_result(&request)
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_definitions()
            .into_iter()
            .find(|tool| tool.name.as_ref() == name)
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
            .with_server_info(Implementation::new("CNshell MCP", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "CNshell only exposes connections explicitly granted in the desktop app. Read operations are bounded; commands and writes require approval.",
            )
    }
}

const MCP_GUIDE: &str = "CNshell MCP 使用说明\n\nMCP 默认关闭，只监听本机 Broker。连接、工具和远端根必须在 CNshell 设置中明确授权；命令、远端写入、删除、上传和下载逐次审批。\n";
const SECURITY_GUIDE: &str = "CNshell MCP 安全边界\n\nsidecar 不接触密码或私钥。请求绑定客户端身份、短期会话、工具和路径授权；本地文件只能使用原生选择器创建的 grant。响应、审计和日志不保存命令输出或文件正文。\n";

fn resource_definitions() -> Vec<Resource> {
    vec![
        Resource::new("cnshell://guide/mcp", "CNshell MCP 使用说明")
            .with_description("CNshell MCP 的工具、审批和生命周期说明")
            .with_mime_type("text/plain"),
        Resource::new("cnshell://guide/security", "CNshell MCP 安全边界")
            .with_description("CNshell MCP 的凭据、路径和审计安全边界")
            .with_mime_type("text/plain"),
        Resource::new("cnshell://connections", "CNshell 已授权连接")
            .with_description("仅列出当前 MCP 客户端获准发现的 SSH 连接")
            .with_mime_type("application/json"),
        Resource::new("cnshell://audit/recent", "CNshell 最近 MCP 审计")
            .with_description("仅返回当前 MCP 客户端最近 100 条脱敏元数据审计")
            .with_mime_type("application/json"),
    ]
}

fn static_resource_result(uri: &str) -> Option<ReadResourceResult> {
    let text = match uri {
        "cnshell://guide/mcp" => MCP_GUIDE,
        "cnshell://guide/security" => SECURITY_GUIDE,
        _ => return None,
    };
    Some(ReadResourceResult::new(vec![ResourceContents::text(
        text, uri,
    )]))
}

fn dynamic_resource_uri(uri: &str) -> bool {
    matches!(uri, "cnshell://connections" | "cnshell://audit/recent")
}

fn dynamic_resource_result(uri: String, value: Value) -> Result<ReadResourceResult, McpError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    if text.len() > MAX_MCP_MESSAGE_BYTES {
        return Err(McpError::internal_error(
            "CNshell MCP resource exceeds 1 MiB",
            None,
        ));
    }
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}

fn prompt_definitions() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "cnshell_diagnose_connection",
            Some("使用 CNshell 已授权的只读工具诊断 SSH 连接"),
            Some(vec![PromptArgument::new("connection").with_required(true)]),
        ),
        Prompt::new(
            "cnshell_review_command",
            Some("在 CNshell 审批前评审远端命令风险"),
            Some(vec![PromptArgument::new("command").with_required(true)]),
        ),
    ]
}

fn prompt_argument(request: &GetPromptRequestParams, name: &str) -> String {
    request
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(name))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 16 * 1024)
        .unwrap_or("未提供")
        .to_string()
}

fn get_prompt_result(request: &GetPromptRequestParams) -> Result<GetPromptResult, McpError> {
    let prompt = match request.name.as_str() {
        "cnshell_diagnose_connection" => {
            let connection = prompt_argument(request, "connection");
            format!(
                "请诊断 CNshell SSH 连接 {connection}。只使用已授权的 CNshell 工具，先读取连接状态和系统信息，再解释可观察到的故障。不要猜测密码、私钥或未授权主机信息。"
            )
        }
        "cnshell_review_command" => {
            let command = prompt_argument(request, "command");
            format!(
                "请对下面的远端命令做安全评审，在 CNshell 审批前说明影响、目标和可逆性；不要执行命令：\n\n{command}"
            )
        }
        _ => return Err(McpError::invalid_params("Unknown CNshell prompt", None)),
    };
    Ok(GetPromptResult::new(vec![PromptMessage::new(
        Role::User,
        ContentBlock::text(prompt),
    )]))
}

fn bounded_result(value: Value, is_error: bool) -> CallToolResult {
    let text = serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"code\":\"serialization_error\",\"message\":\"CNshell MCP response could not be serialized\"}"
            .into()
    });
    let mut result = CallToolResult::success(vec![ContentBlock::text(text.clone())]);
    result.structured_content = Some(value);
    result.is_error = Some(is_error);
    if serde_json::to_vec(&result).is_ok_and(|encoded| encoded.len() <= MAX_MCP_MESSAGE_BYTES) {
        return result;
    }

    // Some MCP Hosts render only content. Keep an interoperable text response when the
    // structured duplicate would exceed the wire limit.
    let mut text_only = CallToolResult::success(vec![ContentBlock::text(text)]);
    text_only.is_error = Some(is_error);
    if serde_json::to_vec(&text_only).is_ok_and(|encoded| encoded.len() <= MAX_MCP_MESSAGE_BYTES) {
        return text_only;
    }

    let overflow_value = json!({
        "code": "response_too_large",
        "message": "CNshell MCP response exceeds 1 MiB"
    });
    let mut overflow = CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string(&overflow_value).expect("fixed JSON serializes"),
    )]);
    overflow.structured_content = Some(overflow_value);
    overflow.is_error = Some(true);
    overflow
}

fn object_schema(properties: Value, required: &[&str]) -> Arc<Map<String, Value>> {
    let Value::Object(properties) = properties else {
        unreachable!();
    };
    Arc::new(Map::from_iter([
        ("type".into(), Value::String("object".into())),
        ("additionalProperties".into(), Value::Bool(false)),
        ("properties".into(), Value::Object(properties)),
        (
            "required".into(),
            Value::Array(
                required
                    .iter()
                    .map(|value| Value::String((*value).into()))
                    .collect(),
            ),
        ),
    ]))
}

fn property(kind: &str, description: &str) -> Value {
    json!({"type": kind, "description": description})
}

fn tool(
    name: &'static str,
    description: &'static str,
    schema: Arc<Map<String, Value>>,
    read_only: bool,
    destructive: bool,
) -> Tool {
    let mut tool = Tool::new(name, description, schema);
    tool.annotations = Some(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(destructive)
            .open_world(true),
    );
    tool
}

fn tool_definitions() -> Vec<Tool> {
    let session = || {
        property(
            "string",
            "Short-lived session ID returned by cnshell_open_session",
        )
    };
    let path = || {
        property(
            "string",
            "Absolute UTF-8 remote path within the granted root",
        )
    };
    vec![
        tool(
            "cnshell_list_connections",
            "List SSH connections explicitly granted to this MCP client. Host and username are hidden unless enabled in CNshell.",
            object_schema(
                json!({
                    "cursor": property("string", "Opaque pagination cursor"),
                    "limit": {"type":"integer","minimum":1,"maximum":100},
                    "tag": property("string", "Optional exact connection tag")
                }),
                &[],
            ),
            true,
            false,
        ),
        tool(
            "cnshell_open_session",
            "Open a short-lived MCP session for an explicitly granted SSH connection.",
            object_schema(
                json!({"connectionId": property("string", "CNshell connection ID")}),
                &["connectionId"],
            ),
            true,
            false,
        ),
        tool(
            "cnshell_close_session",
            "Close a short-lived CNshell MCP session.",
            object_schema(json!({"sessionId": session()}), &["sessionId"]),
            true,
            false,
        ),
        tool(
            "cnshell_file_list",
            "List one page of a remote directory without recursive traversal.",
            object_schema(
                json!({
                    "sessionId": session(), "path": path(),
                    "cursor": property("string", "Opaque pagination cursor"),
                    "limit": {"type":"integer","minimum":1,"maximum":500},
                    "showHidden": property("boolean", "Include hidden entries")
                }),
                &["sessionId", "path"],
            ),
            true,
            false,
        ),
        tool(
            "cnshell_file_read",
            "Read at most 256 KiB of an ordinary UTF-8 remote file. Symlinks and special files are rejected.",
            object_schema(
                json!({
                    "sessionId": session(), "path": path(),
                    "offset": {"type":"integer","minimum":0},
                    "maxBytes": {"type":"integer","minimum":1,"maximum":262144}
                }),
                &["sessionId", "path"],
            ),
            true,
            false,
        ),
        tool(
            "cnshell_system_info",
            "Read bounded OS, CPU, memory, interface and disk information from the SSH host.",
            object_schema(json!({"sessionId": session()}), &["sessionId"]),
            true,
            false,
        ),
        tool(
            "cnshell_run_command",
            "Run one non-interactive SSH command after approval in CNshell. Output is limited to 1 MiB.",
            object_schema(
                json!({
                    "sessionId": session(), "command": {"type":"string","minLength":1,"maxLength":16384},
                    "timeoutSeconds": {"type":"integer","minimum":1,"maximum":600}
                }),
                &["sessionId", "command"],
            ),
            false,
            true,
        ),
        tool(
            "cnshell_file_write",
            "Atomically create or replace an ordinary UTF-8 remote file after approval. Existing files require expectedSha256.",
            object_schema(
                json!({
                    "sessionId": session(), "path": path(),
                    "content": {"type":"string","maxLength":262144},
                    "expectedSha256": property("string", "sha256:<hex> of the current remote file")
                }),
                &["sessionId", "path", "content"],
            ),
            false,
            true,
        ),
        tool(
            "cnshell_file_mkdir",
            "Create one remote directory after approval.",
            object_schema(
                json!({"sessionId": session(), "path": path()}),
                &["sessionId", "path"],
            ),
            false,
            false,
        ),
        tool(
            "cnshell_file_rename",
            "Rename a remote file or directory inside the granted root after approval.",
            object_schema(
                json!({
                    "sessionId": session(), "from": path(), "to": path(),
                    "expectedSha256": property("string", "Required sha256:<hex> for a regular file; omit for a directory")
                }),
                &["sessionId", "from", "to"],
            ),
            false,
            true,
        ),
        tool(
            "cnshell_file_delete",
            "Delete a remote file or directory after approval. Recursive deletion is high risk; grant roots cannot be deleted.",
            object_schema(
                json!({"sessionId": session(), "path": path(), "recursive": property("boolean", "Delete a non-empty directory recursively")}),
                &["sessionId", "path"],
            ),
            false,
            true,
        ),
        tool(
            "cnshell_file_upload",
            "Upload one ordinary local file or directory selected in CNshell to the granted remote root after approval. Absolute local paths are never accepted.",
            object_schema(
                json!({
                    "sessionId": session(),
                    "localGrantId": property("string", "Local upload capability created in CNshell"),
                    "relativePath": property("string", "Relative path inside a granted directory; empty for an exact file grant"),
                    "remotePath": path(),
                    "conflictPolicy": {"type":"string","enum":["overwrite","skip","rename"]}
                }),
                &["sessionId", "localGrantId", "remotePath"],
            ),
            false,
            true,
        ),
        tool(
            "cnshell_file_download",
            "Download one ordinary remote file or directory into a local directory selected in CNshell after approval. Absolute local paths are never accepted.",
            object_schema(
                json!({
                    "sessionId": session(),
                    "remotePath": path(),
                    "localGrantId": property("string", "Local download directory capability created in CNshell"),
                    "relativePath": property("string", "Required relative destination path inside the granted directory"),
                    "conflictPolicy": {"type":"string","enum":["overwrite","skip","rename"]}
                }),
                &["sessionId", "remotePath", "localGrantId", "relativePath"],
            ),
            false,
            true,
        ),
    ]
}

enum SidecarMode {
    Serve {
        client_id: String,
        client_name: String,
        discovery: PathBuf,
    },
    Provision(String),
    Revoke {
        client_id: String,
        expected_sha256: String,
    },
}

fn parse_arguments() -> Result<SidecarMode, String> {
    parse_arguments_from(std::env::args().skip(1))
}

fn parse_arguments_from(
    arguments: impl IntoIterator<Item = String>,
) -> Result<SidecarMode, String> {
    let raw_arguments = arguments.into_iter().collect::<Vec<_>>();
    if let Some(mode) = raw_arguments.first()
        && mode == "--provision-client-secret"
    {
        let client_id = raw_arguments
            .get(1)
            .cloned()
            .ok_or_else(|| format!("missing value for {mode}"))?;
        if raw_arguments.len() != 2 {
            return Err(format!("unexpected arguments after {mode}"));
        }
        return Ok(SidecarMode::Provision(client_id));
    }
    if raw_arguments.first().map(String::as_str) == Some("--revoke-client-secret") {
        if raw_arguments.len() != 4
            || raw_arguments.get(2).map(String::as_str) != Some("--expected-sha256")
        {
            return Err("expected --revoke-client-secret <id> --expected-sha256 <digest>".into());
        }
        return Ok(SidecarMode::Revoke {
            client_id: raw_arguments[1].clone(),
            expected_sha256: raw_arguments[3].clone(),
        });
    }
    let mut arguments = raw_arguments.into_iter();
    let mut client_id = None;
    let mut client_name = None;
    let mut discovery = None;
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--client-id" => client_id = Some(value),
            "--client-name" => client_name = Some(value),
            "--discovery" => discovery = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown argument: {flag}")),
        }
    }
    Ok(SidecarMode::Serve {
        client_id: client_id.ok_or("missing --client-id")?,
        client_name: client_name.ok_or("missing --client-name")?,
        discovery: discovery.ok_or("missing --discovery")?,
    })
}

#[tokio::main]
async fn main() {
    let mode = match parse_arguments() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("CNshell MCP configuration error: {error}");
            std::process::exit(2);
        }
    };
    let (client_id, client_name, discovery) = match mode {
        SidecarMode::Provision(client_id) => {
            match cnshell_lib::mcp::provision_client_secret(&client_id) {
                Ok(digest) => {
                    println!("{digest}");
                    return;
                }
                Err(error) => {
                    eprintln!("CNshell MCP credential provisioning failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        SidecarMode::Revoke {
            client_id,
            expected_sha256,
        } => match cnshell_lib::mcp::revoke_client_secret(&client_id, &expected_sha256) {
            Ok(()) => return,
            Err(error) => {
                eprintln!("CNshell MCP credential revocation failed: {error}");
                std::process::exit(1);
            }
        },
        SidecarMode::Serve {
            client_id,
            client_name,
            discovery,
        } => (client_id, client_name, discovery),
    };
    let server = CnshellMcpServer {
        client_id,
        client_name,
        discovery,
    };
    let input = LimitedLineReader::new(tokio::io::stdin());
    match server.serve((input, tokio::io::stdout())).await {
        Ok(service) => {
            let _ = service.waiting().await;
        }
        Err(error) => {
            eprintln!("CNshell MCP startup failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn credential_modes_require_exact_arguments() {
        assert!(matches!(
            parse_arguments_from(arguments(&["--provision-client-secret", "client-id"])),
            Ok(SidecarMode::Provision(id)) if id == "client-id"
        ));
        assert!(matches!(
            parse_arguments_from(arguments(&[
                "--revoke-client-secret",
                "client-id",
                "--expected-sha256",
                "sha256:digest",
            ])),
            Ok(SidecarMode::Revoke { client_id, expected_sha256 })
                if client_id == "client-id" && expected_sha256 == "sha256:digest"
        ));
        assert!(
            parse_arguments_from(arguments(&["--revoke-client-secret", "client-id",])).is_err()
        );
        assert!(
            parse_arguments_from(arguments(&[
                "--provision-client-secret",
                "client-id",
                "unexpected",
            ]))
            .is_err()
        );
    }

    #[tokio::test]
    async fn limited_reader_accepts_multiple_bounded_messages() {
        let input = b"first\nsecond\n".as_slice();
        let mut reader = LimitedLineReader::new(input);
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();
        assert_eq!(output, input);
    }

    #[tokio::test]
    async fn limited_reader_rejects_an_oversized_message() {
        let input = vec![b'x'; MAX_MCP_MESSAGE_BYTES + 1];
        let mut reader = LimitedLineReader::new(input.as_slice());
        let mut output = Vec::new();
        let error = reader.read_to_end(&mut output).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn oversized_tool_result_becomes_a_bounded_error() {
        let result = bounded_result(json!({"value": "x".repeat(MAX_MCP_MESSAGE_BYTES)}), false);
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.structured_content.unwrap()["code"],
            "response_too_large"
        );
    }

    #[test]
    fn tool_result_keeps_text_and_structured_content_for_host_compatibility() {
        let value = json!({"connectionId":"test","online":true});
        let result = bounded_result(value.clone(), false);
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content, Some(value));
        assert_eq!(result.content.len(), 1);
        let ContentBlock::Text(text) = &result.content[0] else {
            panic!("tool result should contain text");
        };
        assert_eq!(
            serde_json::from_str::<Value>(&text.text).unwrap()["connectionId"],
            "test"
        );
    }

    #[test]
    fn tool_catalog_is_complete_unique_and_strict() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 13);
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), tools.len());
        assert!(tools.iter().all(|tool| {
            tool.input_schema.get("additionalProperties") == Some(&Value::Bool(false))
        }));
    }

    #[test]
    fn resources_and_prompts_are_advertised_and_readable() {
        let server = CnshellMcpServer {
            client_id: "test-client".into(),
            client_name: "Test Client".into(),
            discovery: PathBuf::from("unused.json"),
        };
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.resources.is_some());
        assert!(info.capabilities.prompts.is_some());

        let resources = resource_definitions();
        assert_eq!(resources.len(), 4);
        assert!(static_resource_result("cnshell://guide/mcp").is_some());
        assert!(static_resource_result("cnshell://guide/security").is_some());
        assert!(dynamic_resource_uri("cnshell://connections"));
        assert!(dynamic_resource_uri("cnshell://audit/recent"));

        let prompts = prompt_definitions();
        assert_eq!(prompts.len(), 2);
        for prompt in prompts {
            let request = GetPromptRequestParams::new(prompt.name.to_string());
            assert!(get_prompt_result(&request).is_ok());
        }
    }

    #[test]
    fn unknown_resource_and_prompt_return_protocol_errors() {
        assert!(static_resource_result("cnshell://unknown").is_none());
        assert!(!dynamic_resource_uri("cnshell://unknown"));
        let request = GetPromptRequestParams::new("cnshell_unknown");
        assert!(get_prompt_result(&request).is_err());
    }

    #[test]
    fn dynamic_resources_are_bounded_json() {
        let result = dynamic_resource_result(
            "cnshell://audit/recent".into(),
            json!({"events":[{"tool":"cnshell_file_read","outcome":"completed"}]}),
        )
        .unwrap();
        assert_eq!(result.contents.len(), 1);
        assert!(
            dynamic_resource_result(
                "cnshell://audit/recent".into(),
                json!({"value":"x".repeat(MAX_MCP_MESSAGE_BYTES)}),
            )
            .is_err()
        );
    }
}
