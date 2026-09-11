import {
  CalendarDays,
  Download,
  ExternalLink,
  GitBranch,
  Info,
} from "lucide-react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { appVersion, releaseChannel, releaseDate } from "../../lib/release";
import { latestReleaseUrl } from "./feedback-links";

export function AboutSettings({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const openReleases = () =>
    api
      .openExternal(latestReleaseUrl)
      .catch((error) => onError(errorMessage(error)));

  return (
    <section className="about-settings" aria-labelledby="about-cnshell-heading">
      <div className="about-product">
        <span className="about-product-icon">
          <Info size={22} />
        </span>
        <div>
          <h2 id="about-cnshell-heading">CNshell</h2>
          <p>安全终端、远程文件与服务器监控工作区</p>
        </div>
        <strong>v{appVersion}</strong>
      </div>
      <dl className="about-release-meta">
        <div>
          <dt>
            <GitBranch size={13} />
            更新通道
          </dt>
          <dd>{releaseChannel}</dd>
        </div>
        <div>
          <dt>
            <CalendarDays size={13} />
            发布日期
          </dt>
          <dd>{releaseDate}</dd>
        </div>
        <div>
          <dt>
            <Download size={13} />
            更新方式
          </dt>
          <dd>
            {appVersion.includes("-") ? "手动下载候选版" : "签名自动更新"}
          </dd>
        </div>
      </dl>
      <button className="button secondary" onClick={() => void openReleases()}>
        <ExternalLink size={14} />
        打开版本下载页
      </button>
    </section>
  );
}
