use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

const CONFIG_SIZE_LIMIT: u64 = 64 * 1024;
const SIGNATURE_SIZE_LIMIT: u64 = 16 * 1024;

#[derive(Deserialize)]
struct ReleaseConfig {
    bundle: Option<BundleConfig>,
    plugins: Option<PluginConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleConfig {
    create_updater_artifacts: Option<bool>,
}

#[derive(Deserialize)]
struct PluginConfig {
    updater: Option<UpdaterConfig>,
}

#[derive(Deserialize)]
struct UpdaterConfig {
    endpoints: Option<Vec<String>>,
    pubkey: Option<String>,
}

fn read_regular_file(path: &Path, label: &str, size_limit: Option<u64>) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| format!("{label}不存在或无法读取"))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(format!("{label}必须是非空普通文件"));
    }
    if size_limit.is_some_and(|limit| metadata.len() > limit) {
        return Err(format!("{label}超过允许大小"));
    }
    fs::read(path).map_err(|_| format!("{label}读取失败"))
}

fn decode_text(value: &str, label: &str) -> Result<String, String> {
    let decoded = STANDARD
        .decode(value.trim())
        .map_err(|_| format!("{label}不是有效 Base64"))?;
    String::from_utf8(decoded).map_err(|_| format!("{label}解码后不是 UTF-8 文本"))
}

fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let url =
        reqwest::Url::parse(endpoint).map_err(|_| "updater endpoint 不是有效 URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("updater endpoint 必须是无凭据、无 fragment 的 HTTPS URL".into());
    }
    Ok(())
}

pub fn verify_updater_signature(
    archive_path: &Path,
    signature_path: &Path,
    release_config_path: &Path,
) -> Result<(), String> {
    let archive = read_regular_file(archive_path, "updater 归档", None)?;
    let signature_bytes =
        read_regular_file(signature_path, "updater 签名", Some(SIGNATURE_SIZE_LIMIT))?;
    let config_bytes =
        read_regular_file(release_config_path, "release 配置", Some(CONFIG_SIZE_LIMIT))?;

    let config: ReleaseConfig = serde_json::from_slice(&config_bytes)
        .map_err(|_| "release 配置不是有效 JSON".to_string())?;
    if config
        .bundle
        .and_then(|bundle| bundle.create_updater_artifacts)
        != Some(true)
    {
        return Err("release 配置必须启用 createUpdaterArtifacts".into());
    }

    let updater = config
        .plugins
        .and_then(|plugins| plugins.updater)
        .ok_or_else(|| "release 配置缺少 updater".to_string())?;
    let endpoints = updater
        .endpoints
        .ok_or_else(|| "release 配置缺少 updater endpoint".to_string())?;
    if endpoints.len() != 1 {
        return Err("release 配置必须包含且仅包含一个 updater endpoint".into());
    }
    validate_endpoint(&endpoints[0])?;

    let public_key_text = decode_text(
        updater
            .pubkey
            .as_deref()
            .ok_or_else(|| "release 配置缺少 updater 公钥".to_string())?,
        "updater 公钥",
    )?;
    let public_key =
        PublicKey::decode(&public_key_text).map_err(|_| "updater 公钥格式无效".to_string())?;

    let signature_base64 = std::str::from_utf8(&signature_bytes)
        .map_err(|_| "updater 签名不是 UTF-8 文本".to_string())?;
    let signature_text = decode_text(signature_base64, "updater 签名")?;
    let signature =
        Signature::decode(&signature_text).map_err(|_| "updater 签名格式无效".to_string())?;

    public_key
        .verify(&archive, &signature, true)
        .map_err(|_| "updater 签名与归档或配置公钥不匹配".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    const PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";

    fn fixture(
        archive: &[u8],
    ) -> (
        tempfile::TempDir,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let directory = tempdir().unwrap();
        let archive_path = directory.path().join("CNshell.app.tar.gz");
        let signature_path = directory.path().join("CNshell.app.tar.gz.sig");
        let config_path = directory.path().join("tauri.release.json");
        fs::write(&archive_path, archive).unwrap();
        fs::write(&signature_path, STANDARD.encode(SIGNATURE)).unwrap();
        fs::write(
            &config_path,
            serde_json::to_vec(&json!({
                "bundle": { "createUpdaterArtifacts": true },
                "plugins": {
                    "updater": {
                        "endpoints": ["https://updates.example.test/latest.json"],
                        "pubkey": STANDARD.encode(PUBLIC_KEY),
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        (directory, archive_path, signature_path, config_path)
    }

    #[test]
    fn accepts_the_same_base64_minisign_envelope_as_the_tauri_updater() {
        let (_directory, archive, signature, config) = fixture(b"test");
        verify_updater_signature(&archive, &signature, &config).unwrap();
    }

    #[test]
    fn rejects_an_archive_that_does_not_match_the_signature() {
        let (_directory, archive, signature, config) = fixture(b"Test");
        let error = verify_updater_signature(&archive, &signature, &config).unwrap_err();
        assert!(error.contains("不匹配"));
    }

    #[test]
    fn rejects_a_validly_encoded_but_different_public_key() {
        let (_directory, archive, signature, config) = fixture(b"test");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["plugins"]["updater"]["pubkey"] = json!(STANDARD.encode(
            "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO4",
        ));
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = verify_updater_signature(&archive, &signature, &config).unwrap_err();
        assert!(error.contains("不匹配"));
    }

    #[test]
    fn rejects_an_insecure_updater_endpoint() {
        let (_directory, archive, signature, config) = fixture(b"test");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["plugins"]["updater"]["endpoints"][0] =
            json!("http://updates.example.test/latest.json");
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let error = verify_updater_signature(&archive, &signature, &config).unwrap_err();
        assert!(error.contains("HTTPS"));
    }
}
