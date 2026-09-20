import {
  Download,
  Eye,
  EyeOff,
  KeyRound,
  ShieldAlert,
  Upload,
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { type FormEvent, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";

type PassphraseMode = "export" | "import";

export function ConnectionBackupSettings({
  onChanged,
  onError,
}: {
  onChanged: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [passphraseMode, setPassphraseMode] = useState<PassphraseMode | null>(
    null,
  );
  const [passphrase, setPassphrase] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [showPassphrase, setShowPassphrase] = useState(false);
  const [passphraseError, setPassphraseError] = useState("");
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(
    null,
  );

  const clearPassphrase = () => {
    setPassphrase("");
    setConfirmation("");
    setShowPassphrase(false);
    setPassphraseError("");
  };

  const closePassphraseForm = () => {
    clearPassphrase();
    setPassphraseMode(null);
    setPendingImportPath(null);
  };

  const exportData = async () => {
    if (!api.isDesktop()) {
      onError("导出需要运行桌面版");
      return;
    }
    closePassphraseForm();
    setBusy(true);
    try {
      const path = await save({
        defaultPath: "CNshell-connections.cnshell.json",
      });
      if (!path) return;
      await api.exportConnections(path, false);
      setStatus("已导出连接资料（不含凭据）");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const startEncryptedExport = () => {
    if (!api.isDesktop()) {
      onError("导出需要运行桌面版");
      return;
    }
    clearPassphrase();
    setPendingImportPath(null);
    setPassphraseMode("export");
    setStatus("");
  };

  const exportEncrypted = async (event: FormEvent) => {
    event.preventDefault();
    if (passphrase.length < 8) {
      setPassphraseError("请输入至少 8 位口令");
      return;
    }
    if (passphrase !== confirmation) {
      setPassphraseError("两次输入的口令不一致");
      return;
    }
    setPassphraseError("");
    setBusy(true);
    try {
      const path = await save({
        defaultPath: "CNshell-connections-encrypted.cnshell.json",
      });
      if (!path) return;
      await api.exportConnections(path, true, passphrase);
      closePassphraseForm();
      setStatus("已导出含凭据的加密备份");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const importData = async () => {
    if (!api.isDesktop()) {
      onError("导入需要运行桌面版");
      return;
    }
    closePassphraseForm();
    setBusy(true);
    let path: string | null = null;
    try {
      path = await open({
        multiple: false,
        filters: [{ name: "CNshell 备份", extensions: ["json"] }],
      });
      if (!path) return;
      const count = await api.importConnections(path);
      await onChanged();
      setStatus(`已从备份导入 ${count} 条连接`);
    } catch (error) {
      const message = errorMessage(error);
      if (!message.includes("口令")) {
        onError(message);
        return;
      }
      if (!path) return;
      setPendingImportPath(path);
      setPassphraseMode("import");
      setStatus("");
    } finally {
      setBusy(false);
    }
  };

  const importEncrypted = async (event: FormEvent) => {
    event.preventDefault();
    if (!pendingImportPath) return;
    if (passphrase.length < 8) {
      setPassphraseError("请输入导出时设置的至少 8 位口令");
      return;
    }
    setPassphraseError("");
    setBusy(true);
    try {
      const count = await api.importConnections(pendingImportPath, passphrase);
      await onChanged();
      closePassphraseForm();
      setStatus(`已从加密备份导入 ${count} 条连接`);
    } catch (error) {
      setPassphraseError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="connection-backup-settings" aria-label="连接库备份">
      <h3>
        <ShieldAlert size={16} />
        连接库备份
      </h3>
      <p className="muted-copy">
        普通导出不包含密码。包含凭据的备份使用 Argon2id 与 AES-256-GCM 加密。
      </p>
      <div className="backup-actions">
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void exportData()}
        >
          <Upload size={14} />
          导出连接资料
        </button>
        <button
          className="button secondary"
          disabled={busy}
          onClick={startEncryptedExport}
        >
          <KeyRound size={14} />
          导出含凭据的加密备份
        </button>
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void importData()}
        >
          <Download size={14} />
          导入备份
        </button>
      </div>
      {passphraseMode && (
        <form
          className="backup-passphrase-panel"
          onSubmit={
            passphraseMode === "export" ? exportEncrypted : importEncrypted
          }
        >
          <div className="backup-passphrase-heading">
            <KeyRound size={17} />
            <span>
              <strong>
                {passphraseMode === "export"
                  ? "设置加密备份口令"
                  : "输入加密备份口令"}
              </strong>
              <small>
                {passphraseMode === "export"
                  ? "此口令用于在另一台电脑解密凭据，请妥善保管。"
                  : "请输入在导出这份备份时设置的口令。"}
              </small>
            </span>
          </div>
          <label>
            <span>{passphraseMode === "export" ? "导出口令" : "备份口令"}</span>
            <div className="backup-secret-field">
              <input
                autoFocus
                type={showPassphrase ? "text" : "password"}
                value={passphrase}
                onChange={(event) => {
                  setPassphrase(event.target.value);
                  setPassphraseError("");
                }}
                autoComplete={
                  passphraseMode === "export"
                    ? "new-password"
                    : "current-password"
                }
                aria-invalid={Boolean(passphraseError)}
                aria-describedby="backup-passphrase-help"
              />
              <button
                type="button"
                className="backup-visibility-toggle"
                aria-label={showPassphrase ? "隐藏口令" : "显示口令"}
                onClick={() => setShowPassphrase((current) => !current)}
              >
                {showPassphrase ? <EyeOff size={14} /> : <Eye size={14} />}
              </button>
            </div>
          </label>
          {passphraseMode === "export" && (
            <label>
              <span>确认口令</span>
              <input
                type={showPassphrase ? "text" : "password"}
                value={confirmation}
                onChange={(event) => {
                  setConfirmation(event.target.value);
                  setPassphraseError("");
                }}
                autoComplete="new-password"
                aria-invalid={Boolean(passphraseError)}
              />
            </label>
          )}
          <div id="backup-passphrase-help" className="backup-passphrase-help">
            {passphraseError ? (
              <span role="alert">{passphraseError}</span>
            ) : (
              <small>至少 8 位；CNshell 不会保存或上传此口令。</small>
            )}
          </div>
          <div className="backup-passphrase-actions">
            <button
              type="button"
              className="button secondary"
              disabled={busy}
              onClick={closePassphraseForm}
            >
              取消
            </button>
            <button type="submit" className="button primary" disabled={busy}>
              {busy
                ? "处理中…"
                : passphraseMode === "export"
                  ? "选择保存位置并导出"
                  : "解密并导入"}
            </button>
          </div>
        </form>
      )}
      {status && (
        <p className="muted-copy" role="status">
          {status}
        </p>
      )}
    </section>
  );
}
