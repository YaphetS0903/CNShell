import type { AutomationSchedule } from "../../types";

const weekdayLabels: Record<string, string> = {
  mon: "周一",
  tue: "周二",
  wed: "周三",
  thu: "周四",
  fri: "周五",
  sat: "周六",
  sun: "周日",
};

function formatInterval(seconds: number) {
  if (seconds % 86400 === 0) return `每 ${seconds / 86400} 天`;
  if (seconds % 3600 === 0) return `每 ${seconds / 3600} 小时`;
  if (seconds % 60 === 0) return `每 ${seconds / 60} 分钟`;
  return `每 ${seconds} 秒`;
}

export function formatAutomationSchedule(schedule: AutomationSchedule) {
  if (schedule.scheduleType === "interval") {
    const seconds = Number(schedule.expression);
    return Number.isFinite(seconds) && seconds > 0
      ? formatInterval(seconds)
      : `固定间隔 · ${schedule.expression}`;
  }
  if (schedule.scheduleType === "once") {
    const date = new Date(schedule.expression);
    return Number.isNaN(date.getTime())
      ? `一次执行 · ${schedule.expression}`
      : `一次执行 · ${date.toLocaleString("zh-CN")}`;
  }
  if (schedule.scheduleType === "daily") return `每天 ${schedule.expression}`;
  if (schedule.scheduleType === "weekly") {
    const [weekday, time] = schedule.expression.split("@");
    return `每${weekdayLabels[weekday.toLowerCase()] ?? weekday} ${time ?? ""}`.trim();
  }
  return `Cron · ${schedule.expression}`;
}

export function formatScheduleTime(value: string | null, timeZone: string) {
  if (!value) return null;
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
      timeZone,
    }).format(new Date(value));
  } catch {
    return new Date(value).toLocaleString("zh-CN");
  }
}
