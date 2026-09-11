import { Download, KeyRound, ShieldAlert, Upload } from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";

export function ConnectionBackupSettings({
  onChanged,
  onError,
}: {
  onChanged: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const [status, setStatus] = useState("");
  const exportData = async (includeSecrets: boolean) => {
    if (!api.isDesktop()) {
      onError("导出需要运行桌面版");
      return;
    }
    const path = await save({
      defaultPath: "CNshell-connections.cnshell.json",
    });
    if (!path) return;
    const passphrase = includeSecrets
      ? (prompt("输入至少 8 位导出口令。该口令无法找回。") ?? undefined)
      : undefined;
    try {
      await api.exportConnections(path, includeSecrets, passphrase);
      setStatus(
        includeSecrets
          ? "已导出含凭据的加密备份"
          : "已导出连接资料（不含凭据）",
      );
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const importData = async () => {
    if (!api.isDesktop()) {
      onError("导入需要运行桌面版");
      return;
    }
    const path = await open({
      multiple: false,
      filters: [{ name: "CNshell 备份", extensions: ["json"] }],
    });
    if (!path) return;
    let passphrase: string | undefined;
    try {
      const count = await api.importConnections(path, passphrase);
      await onChanged();
      setStatus(`已从备份导入 ${count} 条连接`);
    } catch (error) {
      const message = errorMessage(error);
      if (!message.includes("口令")) {
        onError(message);
        return;
      }
      passphrase = prompt("该备份已加密，请输入导出口令") ?? undefined;
      try {
        const count = await api.importConnections(path, passphrase);
        await onChanged();
        setStatus(`已从加密备份导入 ${count} 条连接`);
      } catch (retryError) {
        onError(errorMessage(retryError));
      }
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
          onClick={() => void exportData(false)}
        >
          <Upload size={14} />
          导出连接资料
        </button>
        <button
          className="button secondary"
          onClick={() => void exportData(true)}
        >
          <KeyRound size={14} />
          导出含凭据的加密备份
        </button>
        <button className="button secondary" onClick={() => void importData()}>
          <Download size={14} />
          导入备份
        </button>
      </div>
      {status && (
        <p className="muted-copy" role="status">
          {status}
        </p>
      )}
    </section>
  );
}
