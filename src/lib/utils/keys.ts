/** KeyboardEvent -> shortcut text in the format src-tauri/src/shortcuts.rs parses ("Ctrl+Alt+M"). */

const NAMED: Record<string, string> = {
  Escape: "Esc",
  Tab: "Tab",
  Backspace: "Backspace",
  Enter: "Return",
  NumpadEnter: "Num+Enter",
  Insert: "Ins",
  Delete: "Del",
  Pause: "Pause",
  PrintScreen: "Print",
  Home: "Home",
  End: "End",
  ArrowLeft: "Left",
  ArrowUp: "Up",
  ArrowRight: "Right",
  ArrowDown: "Down",
  PageUp: "PgUp",
  PageDown: "PgDown",
  ScrollLock: "ScrollLock",
  Space: "Space",
  Comma: "Comma",
  Minus: "Minus",
  Period: "Period",
  Slash: "Slash",
  Semicolon: "Semicolon",
  Equal: "Equal",
  Quote: "Apostrophe",
  BracketLeft: "BracketLeft",
  Backslash: "Backslash",
  BracketRight: "BracketRight",
  Backquote: "QuoteLeft",
  NumpadAdd: "Num+Plus",
  NumpadSubtract: "Num+Minus",
  NumpadMultiply: "Num+Asterisk",
  NumpadDivide: "Num+Slash",
  NumpadDecimal: "Num+Period",
  AudioVolumeDown: "Volume Down",
  AudioVolumeMute: "Volume Mute",
  AudioVolumeUp: "Volume Up",
  MediaPlayPause: "Toggle Media Play/Pause",
  MediaStop: "Media Stop",
  MediaTrackPrevious: "Media Previous",
  MediaTrackNext: "Media Next",
  MicrophoneMute: "Microphone Mute",
};

const MODIFIER_CODES = new Set([
  "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight", "CapsLock", "NumLock", "AltGraph",
]);

export type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

/** null for a lone modifier (keep waiting) or a key we cannot bind. */
export function keyEventToText(e: KeyLike): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  let key: string | undefined;
  let m: RegExpMatchArray | null;
  if ((m = e.code.match(/^Key([A-Z])$/))) key = m[1];
  else if ((m = e.code.match(/^Digit(\d)$/))) key = m[1];
  else if ((m = e.code.match(/^Numpad(\d)$/))) key = `Num+${m[1]}`;
  else if ((m = e.code.match(/^F(\d{1,2})$/))) key = `F${m[1]}`;
  else key = NAMED[e.code];
  if (!key) return null;
  const mods = [e.metaKey && "Meta", e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean) as string[];
  // "Num" is a modifier in the stored format and goes after the others.
  return [...mods, key].join("+");
}

/** Shorter text for small buttons. */
export function keyLabel(text: string): string {
  return text.replace("Volume ", "Vol ").replace("Toggle Media Play/Pause", "Play/Pause").replace("Microphone Mute", "Mic Mute");
}
