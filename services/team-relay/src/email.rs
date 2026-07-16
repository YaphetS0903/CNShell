use crate::{RelayError, RelayResult};
use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use std::{env, fs, path::Path, sync::Arc};

#[derive(Debug, Clone)]
pub struct VerificationEmail {
    pub recipient: String,
    pub token: String,
    pub expires_at: String,
}

#[async_trait]
pub trait VerificationEmailSender: Send + Sync {
    async fn send_verification(&self, message: VerificationEmail) -> RelayResult<()>;
}

#[derive(Clone)]
pub enum AccountRegistrationMode {
    TrustedLocal,
    RequireEmail(Arc<dyn VerificationEmailSender>),
}

#[derive(Clone)]
pub struct SmtpVerificationEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpVerificationEmailSender {
    pub fn from_env() -> Result<Option<Self>, String> {
        let configured = [
            "CNSHELL_RELAY_SMTP_HOST",
            "CNSHELL_RELAY_SMTP_PORT",
            "CNSHELL_RELAY_SMTP_SECURITY",
            "CNSHELL_RELAY_SMTP_FROM",
            "CNSHELL_RELAY_SMTP_USERNAME",
            "CNSHELL_RELAY_SMTP_PASSWORD",
            "CNSHELL_RELAY_SMTP_PASSWORD_FILE",
        ]
        .iter()
        .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()));
        if !configured {
            return Ok(None);
        }
        let host = optional_env("CNSHELL_RELAY_SMTP_HOST");
        let from = optional_env("CNSHELL_RELAY_SMTP_FROM");
        let host = host.ok_or_else(|| "CNSHELL_RELAY_SMTP_HOST is required".to_string())?;
        let from = from
            .ok_or_else(|| "CNSHELL_RELAY_SMTP_FROM is required".to_string())?
            .parse::<Mailbox>()
            .map_err(|_| "CNSHELL_RELAY_SMTP_FROM is invalid".to_string())?;
        let security = optional_env("CNSHELL_RELAY_SMTP_SECURITY")
            .unwrap_or_else(|| "tls".into())
            .to_ascii_lowercase();
        let mut builder = match security.as_str() {
            "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&host),
            "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host),
            _ => return Err("CNSHELL_RELAY_SMTP_SECURITY must be tls or starttls".into()),
        }
        .map_err(|_| "CNSHELL_RELAY_SMTP_HOST is invalid".to_string())?;
        let default_port = if security == "starttls" { 587 } else { 465 };
        let port = optional_env("CNSHELL_RELAY_SMTP_PORT")
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "CNSHELL_RELAY_SMTP_PORT is invalid".to_string())
            })
            .transpose()?
            .unwrap_or(default_port);
        builder = builder.port(port);
        let username = optional_env("CNSHELL_RELAY_SMTP_USERNAME");
        let password = smtp_password()?;
        match (username, password) {
            (Some(username), Some(password)) => {
                builder = builder.credentials(Credentials::new(username, password));
            }
            (None, None) => {}
            _ => {
                return Err(
                    "CNSHELL_RELAY_SMTP_USERNAME and CNSHELL_RELAY_SMTP_PASSWORD must be set together"
                        .into(),
                );
            }
        }
        Ok(Some(Self {
            transport: builder.build(),
            from,
        }))
    }
}

#[async_trait]
impl VerificationEmailSender for SmtpVerificationEmailSender {
    async fn send_verification(&self, message: VerificationEmail) -> RelayResult<()> {
        let recipient = message
            .recipient
            .parse::<Mailbox>()
            .map_err(|_| RelayError::Validation("邮箱格式无效".into()))?;
        let email = Message::builder()
            .from(self.from.clone())
            .to(recipient)
            .subject("验证你的 CNshell 团队服务邮箱")
            .body(format!(
                "请在 CNshell 的在线团队服务设置中粘贴以下一次性验证令牌：\n\n{}\n\n令牌有效至 {}。如果不是你发起的注册，请忽略此邮件。",
                message.token, message.expires_at
            ))
            .map_err(|_| RelayError::Internal)?;
        self.transport.send(email).await.map_err(|error| {
            tracing::warn!(error = %error, "verification email delivery failed");
            RelayError::Unavailable("验证邮件暂时无法发送，请稍后重试".into())
        })?;
        Ok(())
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn smtp_password() -> Result<Option<String>, String> {
    let direct = env::var("CNSHELL_RELAY_SMTP_PASSWORD")
        .ok()
        .filter(|value| !value.is_empty());
    let file = optional_env("CNSHELL_RELAY_SMTP_PASSWORD_FILE");
    match (direct, file) {
        (Some(_), Some(_)) => Err(
            "CNSHELL_RELAY_SMTP_PASSWORD and CNSHELL_RELAY_SMTP_PASSWORD_FILE are mutually exclusive"
                .into(),
        ),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => {
            if path.len() > 4096 {
                return Err("CNSHELL_RELAY_SMTP_PASSWORD_FILE is invalid".into());
            }
            let path = Path::new(&path);
            if !path.is_absolute() {
                return Err("CNSHELL_RELAY_SMTP_PASSWORD_FILE must be absolute".into());
            }
            let metadata = fs::symlink_metadata(path)
                .map_err(|_| "CNSHELL_RELAY_SMTP_PASSWORD_FILE cannot be read".to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 4096 {
                return Err("CNSHELL_RELAY_SMTP_PASSWORD_FILE must be a small regular file".into());
            }
            let value = fs::read_to_string(path)
                .map_err(|_| "CNSHELL_RELAY_SMTP_PASSWORD_FILE cannot be read".to_string())?;
            let value = value.trim_end_matches(['\r', '\n']).to_string();
            if value.is_empty() {
                return Err("CNSHELL_RELAY_SMTP_PASSWORD_FILE is empty".into());
            }
            Ok(Some(value))
        }
        (None, None) => Ok(None),
    }
}
