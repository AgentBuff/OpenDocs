/**
 * Pure timing policy for local-first autosave. The document engine accepts
 * every keystroke immediately; only network delivery observes this delay.
 */
export const AUTOSAVE_IDLE_DELAY_MS = 800;
export const AUTOSAVE_MAX_LATENCY_MS = 3_000;

export function autosaveDelayMs({
  now,
  firstPendingAt,
  lastEditAt,
}: {
  now: number;
  firstPendingAt: number;
  lastEditAt: number;
}): number {
  const idleRemaining = AUTOSAVE_IDLE_DELAY_MS - (now - lastEditAt);
  const maxRemaining = AUTOSAVE_MAX_LATENCY_MS - (now - firstPendingAt);
  return Math.max(0, Math.min(idleRemaining, maxRemaining));
}
