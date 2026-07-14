import type { FeedbackEnvironment } from "../../lib/api";

const REPOSITORY = "https://github.com/YaphetS0903/CNShell";

export const featureRequestUrl = `${REPOSITORY}/issues/new?template=feature_request.yml`;
export const latestReleaseUrl = `${REPOSITORY}/releases/latest`;

export function bugReportUrl(environment:FeedbackEnvironment|null):string {
  const body=environment?[
    "### CNshell 环境",
    `- 版本：${environment.appVersion}`,
    `- 系统：${environment.operatingSystem} ${environment.osVersion}`,
    `- 架构：${environment.architecture}`,
    "",
    "### 问题描述",
    "<!-- 请描述问题，不要填写主机、IP、用户名、路径、命令输出或凭据。 -->",
    "",
    "### 复现步骤",
    "1. ",
    "",
    "### 预期结果",
    "",
    "### 实际结果",
  ].join("\n"):"";
  const parameters=new URLSearchParams({title:"[Bug] ",body});
  return `${REPOSITORY}/issues/new?${parameters.toString()}`;
}
