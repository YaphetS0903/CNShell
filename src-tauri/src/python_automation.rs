use crate::{
    error::{AppError, AppResult},
    models::{AutomationPlan, AutomationStep, PythonAutomationPreview, PythonAutomationRequest},
};
use rustpython_parser::{Parse, ast};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};
use uuid::Uuid;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_LOCAL_PATHS: usize = 32;
const PERMISSIONS: &[&str] = &[
    "executeCommand",
    "readResults",
    "transferUpload",
    "transferDownload",
];

pub fn compile(request: &PythonAutomationRequest) -> AppResult<PythonAutomationPreview> {
    validate_request(request)?;
    let suite = ast::Suite::parse(&request.source, "<cnshell-automation>")
        .map_err(|error| AppError::Validation(format!("Python 语法错误：{error}")))?;
    if suite.is_empty() || suite.len() > 50 {
        return Err(AppError::Validation(
            "Python 脚本必须包含 1～50 个调用".into(),
        ));
    }
    let permissions: HashSet<&str> = request
        .manifest
        .permissions
        .iter()
        .map(String::as_str)
        .collect();
    let allowed_paths = request
        .manifest
        .allowed_local_paths
        .iter()
        .map(|path| normalize_local_path(path))
        .collect::<AppResult<HashSet<_>>>()?;
    let mut steps = Vec::with_capacity(suite.len());
    let mut warnings = Vec::new();
    for statement in suite {
        let ast::Stmt::Expr(statement) = statement else {
            return Err(AppError::Validation(
                "受限 Python 只允许直接调用 cnshell API；不支持 import、变量、循环、函数或类"
                    .into(),
            ));
        };
        let ast::Expr::Call(call) = *statement.value else {
            return Err(AppError::Validation("每一行必须是 cnshell API 调用".into()));
        };
        let method = method_name(&call)?;
        validate_keywords(&call, method)?;
        let timeout = keyword_u64(&call, "timeout")?.unwrap_or(30);
        if !(1..=3600).contains(&timeout) {
            return Err(AppError::Validation("timeout 必须为 1～3600 秒".into()));
        }
        let mut step = AutomationStep {
            id: Uuid::new_v4().to_string(),
            kind: String::new(),
            command: None,
            pattern: None,
            timeout_seconds: Some(timeout),
            action: None,
            direction: None,
            local_path: None,
            remote_path: None,
        };
        match method {
            "command" => {
                require_permission(&permissions, "executeCommand")?;
                expect_arg_count(&call, 1)?;
                let command = string_arg(&call, 0, "command")?;
                if command.len() > 16 * 1024 || command.contains('\0') {
                    return Err(AppError::Validation("远端命令过长或包含 NUL".into()));
                }
                if dangerous_command(&command) {
                    warnings.push(format!("高风险命令需要再次确认：{command}"));
                }
                step.kind = "command".into();
                step.command = Some(command);
            }
            "wait_for" => {
                require_permission(&permissions, "readResults")?;
                expect_arg_count(&call, 1)?;
                step.kind = "waitForMatch".into();
                step.pattern = Some(pattern_arg(&call, 0)?);
            }
            "require" => {
                require_permission(&permissions, "readResults")?;
                expect_arg_count(&call, 1)?;
                let action =
                    keyword_string(&call, "action")?.unwrap_or_else(|| "continueIfMatch".into());
                if !["continueIfMatch", "stopIfMatch", "stopIfMissing"].contains(&action.as_str()) {
                    return Err(AppError::Validation("require.action 无效".into()));
                }
                step.kind = "condition".into();
                step.pattern = Some(pattern_arg(&call, 0)?);
                step.action = Some(action);
            }
            "upload" | "download" => {
                let permission = if method == "upload" {
                    "transferUpload"
                } else {
                    "transferDownload"
                };
                require_permission(&permissions, permission)?;
                expect_arg_count(&call, 2)?;
                let (local_index, remote_index) = if method == "upload" { (0, 1) } else { (1, 0) };
                let local = normalize_local_path(&string_arg(&call, local_index, "local path")?)?;
                if !allowed_paths.contains(&local) {
                    return Err(AppError::Validation(format!(
                        "本地路径未在 manifest 授权：{local}"
                    )));
                }
                let remote = string_arg(&call, remote_index, "remote path")?;
                if remote.is_empty() || remote.len() > 4096 || remote.contains(['\0', '\n', '\r']) {
                    return Err(AppError::Validation("远端路径无效".into()));
                }
                step.kind = "transfer".into();
                step.direction = Some(method.into());
                step.local_path = Some(local);
                step.remote_path = Some(remote);
            }
            _ => unreachable!(),
        }
        steps.push(step);
    }
    let hash = Sha256::digest(request.source.as_bytes());
    Ok(PythonAutomationPreview {
        script_hash: format!("sha256:{hash:x}"),
        target_connection_id: request.manifest.connection_id.clone(),
        permissions: request.manifest.permissions.clone(),
        steps,
        warnings,
    })
}

pub fn plan(request: &PythonAutomationRequest, preview: PythonAutomationPreview) -> AutomationPlan {
    AutomationPlan {
        id: request.id.clone(),
        name: request.name.clone(),
        connection_id: preview.target_connection_id,
        steps: preview.steps,
    }
}

fn validate_request(request: &PythonAutomationRequest) -> AppResult<()> {
    if request.id.trim().is_empty()
        || request.name.trim().is_empty()
        || request.manifest.connection_id.trim().is_empty()
    {
        return Err(AppError::Validation("脚本名称和目标连接不能为空".into()));
    }
    if request.source.is_empty() || request.source.len() > MAX_SOURCE_BYTES {
        return Err(AppError::Validation("Python 脚本必须为 1～64 KB".into()));
    }
    if request.manifest.permissions.len() > PERMISSIONS.len()
        || request.manifest.allowed_local_paths.len() > MAX_LOCAL_PATHS
    {
        return Err(AppError::Validation("权限或本地路径授权数量超限".into()));
    }
    let mut permissions = HashSet::new();
    for permission in &request.manifest.permissions {
        if !PERMISSIONS.contains(&permission.as_str()) || !permissions.insert(permission.as_str()) {
            return Err(AppError::Validation(format!(
                "未知或重复权限：{permission}"
            )));
        }
    }
    Ok(())
}

fn method_name(call: &ast::ExprCall) -> AppResult<&str> {
    let ast::Expr::Attribute(attribute) = call.func.as_ref() else {
        return Err(AppError::Validation("只允许调用 cnshell.*".into()));
    };
    let ast::Expr::Name(root) = attribute.value.as_ref() else {
        return Err(AppError::Validation("只允许调用 cnshell.*".into()));
    };
    if root.id.as_str() != "cnshell" {
        return Err(AppError::Validation("只允许调用 cnshell.*".into()));
    }
    let method = attribute.attr.as_str();
    if !["command", "wait_for", "require", "upload", "download"].contains(&method) {
        return Err(AppError::Validation(format!(
            "不支持的 cnshell API：{method}"
        )));
    }
    Ok(method)
}

fn validate_keywords(call: &ast::ExprCall, method: &str) -> AppResult<()> {
    let allowed: &[&str] = if method == "require" {
        &["timeout", "action"]
    } else {
        &["timeout"]
    };
    let mut seen = HashSet::new();
    for keyword in &call.keywords {
        let Some(name) = keyword.arg.as_ref().map(|value| value.as_str()) else {
            return Err(AppError::Validation("不允许 **kwargs".into()));
        };
        if !allowed.contains(&name) || !seen.insert(name) {
            return Err(AppError::Validation(format!("不支持或重复的参数：{name}")));
        }
    }
    Ok(())
}

fn expect_arg_count(call: &ast::ExprCall, expected: usize) -> AppResult<()> {
    if call.args.len() != expected {
        return Err(AppError::Validation(format!(
            "该 API 需要 {expected} 个位置参数"
        )));
    }
    Ok(())
}

fn string_arg(call: &ast::ExprCall, index: usize, label: &str) -> AppResult<String> {
    constant_string(
        call.args
            .get(index)
            .ok_or_else(|| AppError::Validation(format!("缺少 {label}")))?,
        label,
    )
}

fn pattern_arg(call: &ast::ExprCall, index: usize) -> AppResult<String> {
    let pattern = string_arg(call, index, "正则表达式")?;
    if pattern.is_empty() || pattern.len() > 512 {
        return Err(AppError::Validation("正则表达式必须为 1～512 字符".into()));
    }
    regex::Regex::new(&pattern)
        .map_err(|error| AppError::Validation(format!("正则表达式无效：{error}")))?;
    Ok(pattern)
}

fn keyword_string(call: &ast::ExprCall, name: &str) -> AppResult<Option<String>> {
    call.keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().map(|arg| arg.as_str()) == Some(name))
        .map(|keyword| constant_string(&keyword.value, name))
        .transpose()
}

fn keyword_u64(call: &ast::ExprCall, name: &str) -> AppResult<Option<u64>> {
    call.keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().map(|arg| arg.as_str()) == Some(name))
        .map(|keyword| match &keyword.value {
            ast::Expr::Constant(value) => match &value.value {
                ast::Constant::Int(value) => value
                    .to_string()
                    .parse::<u64>()
                    .map_err(|_| AppError::Validation(format!("{name} 必须是正整数"))),
                _ => Err(AppError::Validation(format!("{name} 必须是整数"))),
            },
            _ => Err(AppError::Validation(format!("{name} 必须是字面量"))),
        })
        .transpose()
}

fn constant_string(expression: &ast::Expr, label: &str) -> AppResult<String> {
    match expression {
        ast::Expr::Constant(value) => match &value.value {
            ast::Constant::Str(value) => Ok(value.clone()),
            _ => Err(AppError::Validation(format!("{label} 必须是字符串字面量"))),
        },
        _ => Err(AppError::Validation(format!("{label} 必须是字面量"))),
    }
}

fn require_permission(permissions: &HashSet<&str>, permission: &str) -> AppResult<()> {
    if !permissions.contains(permission) {
        return Err(AppError::Validation(format!(
            "manifest 缺少权限：{permission}"
        )));
    }
    Ok(())
}

fn normalize_local_path(value: &str) -> AppResult<String> {
    let path = Path::new(value);
    if !path.is_absolute() || value.len() > 4096 {
        return Err(AppError::Validation("本地授权路径必须是绝对路径".into()));
    }
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(AppError::Validation("本地授权路径不能包含 ..".into()));
    }
    if std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(AppError::Validation("本地授权路径不能是符号链接".into()));
    }
    Ok(path.to_string_lossy().into_owned())
}

fn dangerous_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    ["rm -rf", "mkfs", "shutdown", "reboot", "poweroff", "sudo "]
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: &str, permissions: &[&str]) -> PythonAutomationRequest {
        PythonAutomationRequest {
            id: "python".into(),
            name: "受限脚本".into(),
            source: source.into(),
            manifest: crate::models::PythonAutomationManifest {
                connection_id: "server".into(),
                permissions: permissions.iter().map(|value| (*value).into()).collect(),
                allowed_local_paths: vec!["/tmp/release.tar.gz".into()],
            },
        }
    }

    #[test]
    fn compiles_only_declared_cnshell_calls() {
        let preview = compile(&request(
            "cnshell.command('uname -a', timeout=10)\ncnshell.require('Linux')",
            &["executeCommand", "readResults"],
        ))
        .unwrap();
        assert_eq!(preview.steps.len(), 2);
        assert!(preview.script_hash.starts_with("sha256:"));
        assert_eq!(preview.steps[0].command.as_deref(), Some("uname -a"));
    }

    #[test]
    fn rejects_imports_dynamic_values_and_missing_permissions() {
        assert!(compile(&request("import os", &["executeCommand"])).is_err());
        assert!(
            compile(&request(
                "cnshell.command(open('/etc/passwd').read())",
                &["executeCommand"]
            ))
            .is_err()
        );
        assert!(compile(&request("cnshell.command('id')", &[])).is_err());
    }

    #[test]
    fn transfers_require_exact_manifest_paths() {
        assert!(
            compile(&request(
                "cnshell.upload('/tmp/release.tar.gz', '/tmp/release.tar.gz')",
                &["transferUpload"],
            ))
            .is_ok()
        );
        assert!(
            compile(&request(
                "cnshell.upload('/etc/passwd', '/tmp/passwd')",
                &["transferUpload"],
            ))
            .is_err()
        );
    }
}
