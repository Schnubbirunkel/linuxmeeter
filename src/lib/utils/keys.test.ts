import { describe, expect, it } from "vitest";
import { keyEventToText } from "./keys";

const ev = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("keyEventToText", () => {
  it("uses the modifier order shortcuts.rs formats", () => {
    expect(keyEventToText(ev("KeyM", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+M");
    expect(keyEventToText(ev("F9", { metaKey: true, shiftKey: true }))).toBe("Meta+Shift+F9");
  });
  it("waits on lone modifiers", () => {
    expect(keyEventToText(ev("ControlLeft", { ctrlKey: true }))).toBeNull();
  });
  it("maps keypad and media keys", () => {
    expect(keyEventToText(ev("NumpadAdd", { ctrlKey: true }))).toBe("Ctrl+Num+Plus");
    expect(keyEventToText(ev("Numpad5"))).toBe("Num+5");
    expect(keyEventToText(ev("AudioVolumeMute"))).toBe("Volume Mute");
    expect(keyEventToText(ev("ArrowUp", { altKey: true }))).toBe("Alt+Up");
  });
  it("rejects keys it cannot bind", () => {
    expect(keyEventToText(ev("IntlBackslash"))).toBeNull();
  });
});
