import type { ConnectionProfile } from "../../types";
import { ConnectionBackupSettings } from "./ConnectionBackupSettings";
import { FeedbackSettings } from "./FeedbackSettings";
import { ProxySettings } from "./ProxySettings";

export function AdvancedSettings({ connections, onChanged, onError, includeFeedback = true }: { connections: ConnectionProfile[]; onChanged: () => Promise<void>; onError: (message: string) => void; includeFeedback?: boolean }) {
  return <div className="advanced-settings">
    <ProxySettings connections={connections} onError={onError}/>
    <ConnectionBackupSettings onChanged={onChanged} onError={onError}/>
    {includeFeedback && <FeedbackSettings onError={onError}/>}
  </div>;
}
