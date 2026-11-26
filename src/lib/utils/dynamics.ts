/** What the gate, compressor and limiter are doing right now, from the
 *  pre-DSP level the engine meters (MeterFrame.stripsPre / busesPre).
 *
 *  These are the static curves the plugins follow, without attack/release
 *  timing; the meter ballistics smooth them. Good for "how hard is it
 *  working", not a sample-accurate gain-reduction readout.
 */

/** Peak of a stereo pair, dBFS. */
export const peakOf = (pair: [number, number]): number => Math.max(pair[0], pair[1]);

/** dB the limiter takes off: how far the bus mix (after its fader) goes over the threshold. */
export function limiterReduction(preDb: number, gainDb: number, thresholdDb: number, enabled: boolean): number {
  if (!enabled || preDb <= -80) return 0;
  return Math.max(0, preDb + gainDb - thresholdDb);
}

/** dB the compressor takes off (hard knee). */
export function compReduction(inDb: number, thresholdDb: number, ratio: number, enabled: boolean): number {
  if (!enabled || inDb <= thresholdDb) return 0;
  return (inDb - thresholdDb) * (1 - 1 / Math.max(1, ratio));
}

/** Whether the gate lets the signal through. */
export function gateOpen(inDb: number, thresholdDb: number, enabled: boolean): boolean {
  return !enabled || inDb >= thresholdDb;
}
