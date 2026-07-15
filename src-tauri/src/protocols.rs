use crate::{
    error::{AppError, AppResult},
    models::ProtocolCapability,
};

fn executable(name: &str) -> Option<String> {
    which::which(name)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

pub fn capabilities() -> Vec<ProtocolCapability> {
    let mosh = crate::mosh::helper_path().map(|path| path.to_string_lossy().into_owned());
    let rz = executable("rz");
    let sz = executable("sz");
    let scp = executable("scp");
    let x11 = crate::x11::availability();
    vec![
        ProtocolCapability { id:"zmodem".into(), label:"Zmodem rz/sz".into(), available:rz.is_some()&&sz.is_some(), executable:rz.or(sz), message:"需要本机与远端同时安装 lrzsz；CNshell 仅在检测到完整依赖后启用文件选择和控制序列接管。".into(), security_warning:Some("Zmodem 传输由交互终端发起，文件仍必须通过 macOS 原生选择器授权。".into()) },
        ProtocolCapability { id:"xymodem".into(), label:"Serial X/Ymodem".into(), available:true, executable:None, message:"CNshell 内置 Xmodem 128/1K、Checksum/CRC 与 Ymodem Batch，可在 Serial 文件面板中启动。".into(), security_warning:Some("Xmodem 不传递文件名和真实长度，下载会保留协议块填充；Ymodem 远端文件名只能落在用户选择的目录内。".into()) },
        ProtocolCapability { id:"scp".into(), label:"SCP 降级".into(), available:true, executable:scp, message:"SFTP 子系统不可用时自动通过现有 SSH 会话降级为 SCP，复用 CNshell 认证、代理和主机指纹校验。".into(), security_warning:Some("上传降级只允许明确的覆盖策略；CNshell 不会使用 sshpass，也不会关闭主机指纹校验。".into()) },
        ProtocolCapability { id:"mosh".into(), label:"Mosh 漫游连接".into(), available:mosh.is_some(), executable:mosh, message:"CNshell 内置受管 mosh-client；启用后通过已验证 SSH 启动远端 mosh-server，再使用 UDP 建立可漫游终端。".into(), security_warning:Some("SSH 代理只负责启动远端服务；Mosh UDP 数据必须能从本机直接到达目标服务器。".into()) },
        ProtocolCapability { id:"x11".into(), label:"X11 转发".into(), available:x11.is_ok(), executable:x11.as_ref().ok().cloned(), message:x11.err().unwrap_or_else(||"XQuartz、DISPLAY、xauth 与本地 socket 均可用；可按可信连接启用真实 X11 channel 转发。".into()), security_warning:Some("远端图形程序可访问本地 X Server；CNshell 使用一次性 cookie 隔离，但仍只应对可信主机开放。".into()) },
        ProtocolCapability { id:"agentForwarding".into(), label:"SSH Agent 转发".into(), available:std::env::var_os("SSH_AUTH_SOCK").is_some(), executable:std::env::var("SSH_AUTH_SOCK").ok(), message:"可按连接独立启用，连接建立后通过 SSH 协议请求 Agent 转发。".into(), security_warning:Some("远端 root 或被入侵进程可能借用本机 Agent 签名；只对完全可信主机启用。".into()) },
    ]
}

pub fn validate_options(
    agent_forwarding: bool,
    x11_enabled: bool,
    mosh_enabled: bool,
    port_start: u16,
    port_end: u16,
) -> AppResult<()> {
    if agent_forwarding && std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err(AppError::Unavailable(
            "当前进程没有 SSH_AUTH_SOCK，无法启用 Agent 转发".into(),
        ));
    }
    if mosh_enabled {
        if !crate::mosh::available() {
            return Err(AppError::Unavailable(
                "CNshell 内置的 mosh-client 缺失或损坏，请重新安装 CNshell".into(),
            ));
        }
        crate::mosh::validate_ports(port_start, port_end)?;
    }
    if x11_enabled {
        crate::x11::availability().map_err(AppError::Unavailable)?;
        if mosh_enabled {
            return Err(AppError::Validation(
                "Mosh 会话不承载 X11 channel，不能与 X11 转发同时启用".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_report_is_explicit_and_unique() {
        let report = capabilities();
        assert_eq!(report.len(), 6);
        let ids = report
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), report.len());
        assert!(report.iter().all(|item| !item.message.is_empty()));
    }
}
