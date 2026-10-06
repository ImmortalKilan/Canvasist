import type { MessageKey } from "./en";

// Typed against the English catalog, so a missing or extra key fails the build.
export const zhCN: Record<MessageKey, string> = {
  "header.refresh": "刷新",
  "header.refreshUnavailable": "连接 Canvas 后即可刷新",
  "header.settings": "设置",

  "empty.title": "尚未连接 Canvas",
  "empty.body":
    "连接你学校的 Canvas 账户，即可在一个地方看到所有 Canvas 与 Gradescope 的截止时间。",
  "empty.connect": "连接 Canvas",
  "empty.comingSoon": "将在下一个版本中提供",

  "settings.title": "设置",
  "settings.back": "返回",
  "settings.language": "语言",
  "settings.language.system": "跟随系统",
  "settings.autostart": "登录 Windows 时自动启动",
  "settings.autostart.hint": "Canvasist 会安静地运行在系统托盘中，确保提醒正常工作。",
  "settings.about": "关于",
  "settings.version": "版本 {version}",

  "error.generic": "出现错误：{message}",
};
