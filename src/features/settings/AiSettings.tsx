import { Save, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { AiProviderProfile } from "../../types";
import { usePlatformCapabilities } from "../../lib/platform";
import { useSettingsModuleDraftState } from "./settings-module-draft";

const DEFAULT_ENDPOINT = "https://api.openai.com/v1";

export function AiSettings({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const platform = usePlatformCapabilities();
  const [providers, setProviders] = useState<AiProviderProfile[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [endpoint, setEndpoint] = useState(DEFAULT_ENDPOINT);
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const selectedProvider = providers.find(
    (provider) => provider.id === selectedId,
  );
  const providerDraftDirty = useMemo(
    () =>
      selectedProvider
        ? name !== selectedProvider.name ||
          endpoint !== selectedProvider.endpoint ||
          model !== selectedProvider.model ||
          Boolean(apiKey)
        : Boolean(name || model || apiKey || endpoint !== DEFAULT_ENDPOINT),
    [apiKey, endpoint, model, name, selectedProvider],
  );
  useSettingsModuleDraftState(providerDraftDirty);

  const select = (provider: AiProviderProfile) => {
    setSelectedId(provider.id);
    setName(provider.name);
    setEndpoint(provider.endpoint);
    setModel(provider.model);
    setApiKey("");
  };
  useEffect(() => {
    void api
      .listAiProviders()
      .then((items) => {
        setProviders(items);
        if (items[0]) select(items[0]);
      })
      .catch((error) => onError(errorMessage(error)));
  }, [onError]);
  const save = async () => {
    try {
      const saved = await api.saveAiProvider({
        id: selectedId || crypto.randomUUID(),
        name,
        endpoint,
        model,
        apiKey: apiKey || null,
      });
      setProviders((current) => [
        ...current.filter((item) => item.id !== saved.id),
        saved,
      ]);
      select(saved);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const remove = async () => {
    if (
      !selectedId ||
      !confirm(
        `删除 AI Provider 配置及${platform.credentialStoreName} API Key？`,
      )
    )
      return;
    try {
      await api.deleteAiProvider(selectedId);
      setProviders((current) =>
        current.filter((item) => item.id !== selectedId),
      );
      setSelectedId("");
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  return (
    <section className="ai-settings" aria-label="AI 辅助">
      <div className="section-heading">
        <div>
          <h3>
            <Sparkles size={16} /> AI Provider
          </h3>
          <p>管理兼容服务、模型和保存在系统凭据库中的 API Key。</p>
        </div>
      </div>
      <div className="ai-providers">
        {providers.map((provider) => (
          <button
            key={provider.id}
            className={provider.id === selectedId ? "active" : ""}
            onClick={() => select(provider)}
          >
            <strong>{provider.name}</strong>
            <small>
              {provider.endpoint} · {provider.model} ·{" "}
              {provider.hasApiKey ? "已配置 Key" : "无 Key"}
            </small>
          </button>
        ))}
      </div>
      <div className="automation-meta">
        <label>
          <span>Provider 名称</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label>
          <span>兼容 endpoint</span>
          <input
            value={endpoint}
            onChange={(event) => setEndpoint(event.target.value)}
          />
        </label>
        <label>
          <span>模型</span>
          <input
            value={model}
            onChange={(event) => setModel(event.target.value)}
            placeholder="由 Provider 提供"
          />
        </label>
        <label>
          <span>API Key</span>
          <input
            type="password"
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            placeholder={selectedId ? "留空保持原 Key" : "可选，本地端点可不填"}
          />
        </label>
      </div>
      <div className="ai-actions">
        <button className="button secondary" onClick={() => void save()}>
          <Save size={14} /> 保存 Provider
        </button>
        <IconButton
          icon={Trash2}
          label="删除 AI Provider"
          disabled={!selectedId}
          onClick={() => void remove()}
        />
      </div>
    </section>
  );
}
