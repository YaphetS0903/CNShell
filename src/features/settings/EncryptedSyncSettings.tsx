import { open } from "@tauri-apps/plugin-dialog";
import { CloudCog, Fingerprint, FolderOpen, LockKeyhole, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { SyncOptions, SyncResult, TouchIdSyncStatus } from "../../types";
import { usePlatformCapabilities } from "../../lib/platform";

export function EncryptedSyncSettings({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const platform = usePlatformCapabilities();
  const biometric = platform.biometricName;
  const [folder, setFolder] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [options, setOptions] = useState<SyncOptions>({
    includeHosts: true,
    includePrivateKeyPaths: false,
    includeCredentials: false,
  });
  const [result, setResult] = useState<SyncResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [touchBusy, setTouchBusy] = useState(false);
  const [touchStatus, setTouchStatus] = useState<TouchIdSyncStatus | null>(null);
  useEffect(() => {
    if (!folder) {
      setTouchStatus(null);
      return;
    }
    let active = true;
    void api.touchIdSyncStatus(folder).then((status) => {
      if (active) setTouchStatus(status);
    }).catch((error) => {
      if (active) {
        setTouchStatus(null);
        onError(errorMessage(error));
      }
    });
    return () => { active = false; };
  }, [folder, onError]);
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
        `凭据将从${platform.credentialStoreName}读取，并在本机加密后写入同步包。服务端只看到密文。确认继续？`,
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
  const touchSync = async () => {
    if (
      options.includeCredentials &&
      !confirm(`凭据将从${platform.credentialStoreName}读取，并在本机加密后写入同步包。服务端只看到密文。确认继续？`)
    ) return;
    setTouchBusy(true);
    try {
      setResult(await api.writeEncryptedSyncWithTouchId(folder, options));
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setTouchBusy(false);
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
  const touchPull = async () => {
    if (!confirm(`使用${biometric}解锁并从同步目录导入连接？同 ID 的本地连接不会被覆盖，将保留为双方冲突副本。`)) return;
    setTouchBusy(true);
    try {
      setResult(await api.readEncryptedSyncWithTouchId(folder));
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setTouchBusy(false);
    }
  };
  const saveTouchId = async () => {
    setTouchBusy(true);
    try {
      setTouchStatus(await api.saveTouchIdSyncKey(folder, passphrase));
      setPassphrase("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setTouchBusy(false);
    }
  };
  const deleteTouchId = async () => {
    if (!confirm(`移除此同步文件夹保存的${biometric}口令？现有加密同步包不会被删除，之后仍可用手动口令恢复。`)) return;
    setTouchBusy(true);
    try {
      await api.deleteTouchIdSyncKey(folder);
      setTouchStatus(await api.touchIdSyncStatus(folder));
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setTouchBusy(false);
    }
  };
  return (
    <section className="encrypted-sync">
      <div className="section-heading">
        <div>
          <h3>
            <CloudCog size={16} />
            本地目录加密同步
          </h3>
          <p>
            选择 iCloud Drive、WebDAV 挂载点或 Git 检出目录。
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
        <span>同步口令（至少 8 位；默认不保存）</span>
        <input
          type="password"
          value={passphrase}
          onChange={(event) => setPassphrase(event.target.value)}
          autoComplete="new-password"
        />
      </label>
      {folder && touchStatus && <div className={`touch-id-vault ${touchStatus.supported ? "available" : "unavailable"}`}>
        <div><Fingerprint size={17}/><span><strong>{biometric}本地保护</strong><small>{touchStatus.message}</small></span></div>
        <div className="backup-actions">
          {!touchStatus.saved && <button className="button secondary" disabled={!touchStatus.supported || passphrase.length < 8 || busy || touchBusy} onClick={()=>void saveTouchId()}><Fingerprint size={14}/>{touchBusy?"处理中…":`用${biometric}保存当前口令`}</button>}
          {touchStatus.saved && <button className="button secondary danger" disabled={busy || touchBusy} onClick={()=>void deleteTouchId()}><Trash2 size={14}/>移除已保存口令</button>}
        </div>
        <small>口令仅限这台电脑使用，并受当前系统生物识别策略保护；验证失败时可继续使用上方手动口令。</small>
      </div>}
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
          <span>同步{platform.credentialStoreName}凭据（默认关闭）</span>
        </label>
      </div>
      <div className="backup-actions">
        <button
          className="button secondary"
          disabled={!folder || passphrase.length < 8 || busy}
          onClick={() => void sync()}
        >
          <LockKeyhole size={14} />
          {busy ? "处理中…" : "用手动口令生成"}
        </button>
        <button
          className="button secondary"
          disabled={!folder || passphrase.length < 8 || busy}
          onClick={() => void pull()}
        >
          用手动口令导入
        </button>
        {touchStatus?.saved && <button className="button secondary" disabled={!touchStatus.supported || busy || touchBusy} onClick={()=>void touchSync()}><Fingerprint size={14}/>{touchBusy?`等待${biometric}…`:`用${biometric}生成`}</button>}
        {touchStatus?.saved && <button className="button secondary" disabled={!touchStatus.supported || busy || touchBusy} onClick={()=>void touchPull()}>用{biometric}导入</button>}
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
