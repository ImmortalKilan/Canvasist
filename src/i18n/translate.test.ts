import { describe, expect, it } from "vitest";
import { en } from "./en";
import { zhCN } from "./zh-CN";
import { translate } from "./translate";

describe("translate", () => {
  it("returns the message for the requested locale", () => {
    expect(translate("en", "settings.title")).toBe("Settings");
    expect(translate("zh-CN", "settings.title")).toBe("设置");
  });

  it("fills placeholders", () => {
    expect(translate("en", "settings.version", { version: "1.2.3" })).toBe("Version 1.2.3");
  });

  it("leaves unknown placeholders intact", () => {
    expect(translate("en", "settings.version", {})).toBe("Version {version}");
  });
});

describe("catalogs", () => {
  it("define the same placeholders in every locale", () => {
    const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const key of Object.keys(en) as (keyof typeof en)[]) {
      expect(placeholders(zhCN[key]), key).toEqual(placeholders(en[key]));
    }
  });

  it("have no empty messages", () => {
    for (const catalog of [en, zhCN]) {
      for (const [key, value] of Object.entries(catalog)) {
        expect(value.trim(), key).not.toBe("");
      }
    }
  });
});
