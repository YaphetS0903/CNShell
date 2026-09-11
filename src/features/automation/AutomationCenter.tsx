import { Modal } from "../../components/Modal";
import type { ConnectionProfile } from "../../types";
import { AutomationSettings } from "../settings/AutomationSettings";
import { useState } from "react";

export default function AutomationCenter({
  open,
  connections,
  onClose,
  onError,
}: {
  open: boolean;
  connections: ConnectionProfile[];
  onClose: () => void;
  onError: (message: string) => void;
}) {
  const [dirty, setDirty] = useState(false);
  if (!open) return null;
  const requestClose = () => {
    if (dirty && !confirm("自动化计划有尚未保存的修改，确定放弃并关闭吗？"))
      return;
    onClose();
  };
  return (
    <Modal
      title="自动化中心"
      onClose={requestClose}
      wide
      dialogClassName="automation-center-dialog"
      bodyClassName="automation-center-body"
    >
      <AutomationSettings
        connections={connections}
        onError={onError}
        onDirtyChange={setDirty}
      />
    </Modal>
  );
}
