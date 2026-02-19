const DB_FLOOR = -60;

/** Convert an RMS amplitude (0..1 float range) to a 0..1 meter level using dB scale. */
export function rmsToLevel(rms: number): number {
  const db = rms > 0 ? 20 * Math.log10(rms) : -100;
  return Math.max(0, Math.min(1, (db - DB_FLOOR) / -DB_FLOOR));
}

/**
 * Apply peak-hold smoothing: fast attack (jump to peaks), slow decay.
 * `prev` is the current displayed level, `level` is the new incoming level.
 */
export function smoothLevel(prev: number, level: number): number {
  return level > prev ? level : prev * 0.92 + level * 0.08;
}
