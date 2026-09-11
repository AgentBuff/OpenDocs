import type { PlaybackState } from "./playback-state.js";

export const PRESENTER_MESSAGE_VERSION = 1;

export interface PresenterCursorMessage {
  readonly version: typeof PRESENTER_MESSAGE_VERSION;
  readonly type: "cursor";
  readonly sessionId: string;
  readonly artifactId: string;
  readonly revision: number;
  readonly cursor: PlaybackState;
}

export interface PresenterHelloMessage {
  readonly version: typeof PRESENTER_MESSAGE_VERSION;
  readonly type: "hello";
  readonly sessionId: string;
  readonly artifactId: string;
}

export type PresenterMessage = PresenterCursorMessage | PresenterHelloMessage;

export function presenterChannelName(sessionId: string): string {
  return `open-office:presentation:${sessionId}`;
}

export function presenterAudienceUrl(currentHref: string, artifactId: string, sessionId: string): string {
  const url = new URL(currentHref);
  url.searchParams.set("doc", artifactId);
  url.searchParams.set("presentationMode", "audience");
  url.searchParams.set("presentationSession", sessionId);
  return url.toString();
}

export function parsePresenterMessage(value: unknown): PresenterMessage | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record.version !== PRESENTER_MESSAGE_VERSION || !nonEmpty(record.sessionId) || !nonEmpty(record.artifactId)) return null;
  if (record.type === "hello") {
    if (!hasOnlyKeys(record, ["version", "type", "sessionId", "artifactId"])) return null;
    return { version: PRESENTER_MESSAGE_VERSION, type: "hello", sessionId: record.sessionId, artifactId: record.artifactId };
  }
  if (record.type !== "cursor" || !isPlaybackState(record.cursor) || !isRevision(record.revision)) return null;
  if (!hasOnlyKeys(record, ["version", "type", "sessionId", "artifactId", "revision", "cursor"])) return null;
  return {
    version: PRESENTER_MESSAGE_VERSION,
    type: "cursor",
    sessionId: record.sessionId,
    artifactId: record.artifactId,
    revision: record.revision,
    cursor: record.cursor,
  };
}

function isPlaybackState(value: unknown): value is PlaybackState {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const cursor = value as Record<string, unknown>;
  return hasOnlyKeys(cursor, ["slideId", "cueId", "elapsedMs", "status"])
    && (cursor.slideId === null || nonEmpty(cursor.slideId))
    && (cursor.cueId === null || nonEmpty(cursor.cueId))
    && typeof cursor.elapsedMs === "number"
    && Number.isSafeInteger(cursor.elapsedMs)
    && cursor.elapsedMs >= 0
    && (cursor.status === "playing" || cursor.status === "paused");
}

function isRevision(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function nonEmpty(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function hasOnlyKeys(record: Record<string, unknown>, allowed: readonly string[]): boolean {
  const keys = Object.keys(record);
  return keys.length === allowed.length && keys.every((key) => allowed.includes(key));
}
