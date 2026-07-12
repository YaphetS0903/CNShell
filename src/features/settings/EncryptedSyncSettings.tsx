import { open } from "@tauri-apps/plugin-dialog";
import { CloudCog, FolderOpen, LockKeyhole } from "lucide-react";
import { useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { SyncOptions, SyncResult } from "../../types";

export function EncryptedSyncSettings({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const [folder, setFolder] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [options, setOptions] = useState<SyncOptions>({
    includeHosts: true,
    includePrivateKeyPaths: false,
    includeCredentials: false,
  });
  const [result, setResult] = useState<SyncResult | null>(null);
  const [busy, setBusy] = useState(false);
  const choose = async () => {
    const path = await open({
      directory: true,
      multiple: false,
      title: "选择同步文件夹",
    });
    if (path) setFolder(path);
  };
  const sync = async () => {
    if (
      options.includeCredentials &&
      !confirm(
        "凭据将从 macOS Keychain 读取，并在本机加密后写入同步包。服务端只看到密文。确认继续？",
      )
    )
      return;
    setBusy(true);
    try {
      setResult(await api.writeEncryptedSync(folder, passphrase, options));
      setPassphrase("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  const pull = async () => {
    if (
      !confirm(
        "从同步目录导入连接？同 ID 的本地连接不会被覆盖，将保留为双方冲突副本。",
      )
    )
      return;
    setBusy(true);
    try {
      setResult(await api.readEncryptedSync(folder, passphrase));
      setPassphrase("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="encrypted-sync">
      <div className="section-heading">
        <div>
          <h3>
            <CloudCog size={16} />
            可选加密同步
          </h3>
          <p>
            选择 iCloud Drive、WebDAV 或 Git 的本地文件夹；CNshell
            不连接第三方账号。
          </p>
        </div>
      </div>
      <label>
        <span>同步文件夹</span>
        <div className="path-picker">
          <input value={folder} readOnly placeholder="选择本地挂载目录" />
          <button className="mini-button" onClick={() => void choose()}>
            <FolderOpen size={12} />
            选择
          </button>
        </div>
      </label>
      <label>
        <span>同步口令（至少 8 位，不保存）</span>
        <input
          type="password"
          value={passphrase}
          onChange={(event) => setPassphrase(event.target.value)}
          autoComplete="new-password"
        />
      </label>
      <div className="sync-toggles">
        <label className="check-row">
          <input
            type="checkbox"
            checked={options.includeHosts}
            onChange={(event) =>
              setOptions({ ...options, includeHosts: event.target.checked })
            }
          />
          <span>同步主机资料</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={options.includePrivateKeyPaths}
            disabled={!options.includeHosts}
            onChange={(event) =>
              setOptions({
                ...options,
                includePrivateKeyPaths: event.target.checked,
              })
            }
          />
          <span>同步私钥路径（不包含私钥内容）</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={options.includeCredentials}
            disabled={!options.includeHosts}
            onChange={(event) =>
              setOptions({
                ...options,
                includeCredentials: event.target.checked,
              })
            }
          />
          <span>同步 Keychain 凭据（默认关闭）</span>
        </label>
      </div>
      <div className="backup-actions">
        <button
          className="button secondary"
          disabled={!folder || passphrase.length < 8 || busy}
          onClick={() => void sync()}
        >
          <LockKeyhole size={14} />
          {busy ? "处理中…" : "生成加密同步包"}
        </button>
        <button
          className="button secondary"
          disabled={!folder || passphrase.length < 8 || busy}
          onClick={() => void pull()}
        >
          导入并保留冲突副本
        </button>
      </div>
      {result && (
        <div className="sync-result">
          <strong>已处理 {result.connectionCount} 个连接</strong>
          <code>{result.path}</code>
          {result.conflictCopy && (
            <small>检测到旧版本，已保留冲突副本：{result.conflictCopy}</small>
          )}
        </div>
      )}
    </section>
  );
}
