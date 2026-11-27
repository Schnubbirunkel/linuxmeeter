import type { BusId } from "../types";

/** Transient UI state that isn't mixer state (open panels, etc.). */

class UiStore {
  /** Strip id whose EQ panel is open, or null. */
  eqStrip = $state<number | null>(null);
  /** A bus whose output EQ panel is open, or null. */
  eqBus = $state<BusId | null>(null);
  /** Strip id whose Dynamics (gate/comp) panel is open, or null. */
  dynStrip = $state<number | null>(null);
  /** Settings panel (notifications, global shortcuts). */
  settingsOpen = $state(false);
}

export const ui = new UiStore();
