import type { PresentationPresenceParticipant } from "@open-office/schema/api";

export interface MindmapRevisionNotice {
  artifactId: string;
  revision: number;
  changedEntities: readonly string[];
  structureChanged: boolean;
}

export function parseMindmapRevisionNotice(
  data: string,
  artifactId: string,
  currentRevision: number,
): MindmapRevisionNotice | null {
  try {
    const value = JSON.parse(data) as Record<string, unknown>;
    if (value.artifactId !== artifactId
      || !Number.isSafeInteger(value.revision)
      || (value.revision as number) <= currentRevision
      || !Array.isArray(value.changedEntities)
      || !value.changedEntities.every((item) => typeof item === "string")
      || typeof value.structureChanged !== "boolean") return null;
    return {
      artifactId,
      revision: value.revision as number,
      changedEntities: value.changedEntities as string[],
      structureChanged: value.structureChanged,
    };
  } catch {
    return null;
  }
}

export function parseMindmapPresence(
  data: string,
  artifactId: string,
  localSessionId: string,
): readonly PresentationPresenceParticipant[] | null {
  try {
    const value = JSON.parse(data) as Record<string, unknown>;
    if (value.artifactId !== artifactId || !Array.isArray(value.participants)) return null;
    const participants = value.participants.filter(isPresenceParticipant);
    if (participants.length !== value.participants.length) return null;
    return participants.filter((participant) => participant.sessionId !== localSessionId);
  } catch {
    return null;
  }
}

function isPresenceParticipant(value: unknown): value is PresentationPresenceParticipant {
  if (!value || typeof value !== "object") return false;
  const participant = value as Record<string, unknown>;
  return typeof participant.sessionId === "string"
    && typeof participant.actorId === "string"
    && typeof participant.displayName === "string"
    && (!participant.selectedNodeIds || (Array.isArray(participant.selectedNodeIds) && participant.selectedNodeIds.every((id) => typeof id === "string")))
    && (!participant.cursor || (typeof participant.cursor === "object" && Number.isFinite((participant.cursor as Record<string, unknown>).x) && Number.isFinite((participant.cursor as Record<string, unknown>).y)));
}
