import { useCallback, useEffect, useRef, useState } from "react";

import { OpenOfficeSdk, isVersionConflict, type SemanticCommandInput } from "@open-office/sdk";
import type {
  ArtifactTransactionHistoryState,
  PresentationDeckProjection,
  PresentationPresenceParticipant,
  PresentationSlideOutlineItem,
  PresentationSlideProjection,
} from "@open-office/schema/api";

import { presentationHistoryTransaction } from "./commands.js";
import { thumbnailInvalidationIds } from "./PresentationThumbnails.js";

const sdk = new OpenOfficeSdk();
const ACTOR_ID = "presentation-web";

export type PresentationStudioData = {
  deck: PresentationDeckProjection;
  slides: PresentationSlideOutlineItem[];
  activeSlide: PresentationSlideProjection | null;
  history: ArtifactTransactionHistoryState;
  revision: number;
};

export type PresentationTransactionOrigin = "local" | "undo" | "redo";

export type PresentationSubmit = (
  commands: readonly SemanticCommandInput[],
  origin?: PresentationTransactionOrigin,
  preferredSlideId?: string | null,
) => Promise<boolean>;

export function presentationErrorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}

export function usePresentationSession(
  artifactId: string,
  selectedNodeIds: readonly string[],
) {
  const [data, setData] = useState<PresentationStudioData | null>(null);
  const [activeSlideId, setActiveSlideId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [availableCapabilities, setAvailableCapabilities] = useState<ReadonlySet<string>>(() => new Set());
  const [capabilitiesLoaded, setCapabilitiesLoaded] = useState(false);
  const [thumbnailDirtyIds, setThumbnailDirtyIds] = useState<readonly string[]>([]);
  const [remotePresence, setRemotePresence] = useState<readonly PresentationPresenceParticipant[]>([]);
  const presenceSessionId = useRef(createPresenceSessionId());
  const presenceCursor = useRef<{ x: number; y: number } | undefined>(undefined);
  const presenceState = useRef<{ slideId: string | null; selectedNodeIds: readonly string[] }>({
    slideId: null,
    selectedNodeIds: [],
  });
  const presenceTimer = useRef<number | null>(null);

  presenceState.current = { slideId: activeSlideId, selectedNodeIds };

  const publishPresence = useCallback(() => {
    if (presenceTimer.current !== null) window.clearTimeout(presenceTimer.current);
    presenceTimer.current = window.setTimeout(() => {
      presenceTimer.current = null;
      const current = presenceState.current;
      void sdk.updatePresentationPresence(artifactId, presenceSessionId.current, {
        ...(current.slideId ? { slideId: current.slideId } : {}),
        selectedNodeIds: [...current.selectedNodeIds],
        ...(presenceCursor.current ? { cursor: presenceCursor.current } : {}),
      }).catch(() => undefined);
    }, 100);
  }, [artifactId]);

  useEffect(() => {
    publishPresence();
    return () => {
      if (presenceTimer.current !== null) window.clearTimeout(presenceTimer.current);
    };
  }, [activeSlideId, publishPresence, selectedNodeIds]);

  useEffect(() => {
    let disposed = false;
    const read = () => {
      void sdk.presentationPresence(artifactId).then((page) => {
        if (!disposed) {
          setRemotePresence(page.participants.filter(
            (participant) => participant.sessionId !== presenceSessionId.current,
          ));
        }
      }).catch(() => undefined);
    };
    read();
    const timer = window.setInterval(read, 2_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [artifactId]);

  const refresh = useCallback(async (preferredSlideId?: string | null) => {
    const [deckEnvelope, outlineEnvelope, history] = await Promise.all([
      sdk.presentation(artifactId),
      sdk.presentationOutline(artifactId, { limit: 200, maxBytes: 256_000 }),
      sdk.history(artifactId),
    ]);
    const slides = outlineEnvelope.data.items;
    const nextSlideId = preferredSlideId ?? activeSlideId ?? slides[0]?.slideId ?? null;
    const activeSlide = nextSlideId
      ? (await sdk.presentationSlide(artifactId, nextSlideId, {
          include: ["nodes", "notes", "timeline"],
          maxBytes: 512_000,
        })).data
      : null;
    setData({
      deck: deckEnvelope.data,
      slides,
      activeSlide,
      history,
      revision: Math.max(deckEnvelope.revision, outlineEnvelope.revision),
    });
    setActiveSlideId(nextSlideId);
  }, [activeSlideId, artifactId]);

  useEffect(() => {
    void refresh().catch((reason: unknown) => setError(presentationErrorMessage(reason)));
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    void sdk.capabilities().then((catalog) => {
      if (disposed) return;
      const presentation = catalog.artifacts.find((artifact) => artifact.kind === "presentation");
      setAvailableCapabilities(new Set(
        presentation?.commands.map((command) => command.typeId) ?? [],
      ));
      setCapabilitiesLoaded(true);
    }).catch((reason: unknown) => {
      if (!disposed) {
        setCapabilitiesLoaded(true);
        setError(presentationErrorMessage(reason));
      }
    });
    return () => {
      disposed = true;
    };
  }, []);

  const submit = useCallback(async (
    commands: readonly SemanticCommandInput[],
    origin: PresentationTransactionOrigin = "local",
    preferredSlideId: string | null = activeSlideId,
  ) => {
    if (!data || commands.length === 0) return false;
    setSaving(true);
    setError(null);
    try {
      const result = await sdk.submit({
        artifactId,
        baseRevision: data.revision,
        actorId: ACTOR_ID,
        origin,
        commands,
      });
      setThumbnailDirtyIds(thumbnailInvalidationIds(result.events));
      await refresh(preferredSlideId);
      setData((current) => current ? {
        ...current,
        history: { canUndo: result.canUndo, canRedo: result.canRedo },
      } : current);
      return true;
    } catch (reason) {
      if (isVersionConflict(reason)) {
        await refresh(activeSlideId);
        setError("此演示文稿已更新，已按最新 revision/ETag 刷新；请重新执行操作。");
      } else {
        setError(presentationErrorMessage(reason));
      }
      return false;
    } finally {
      setSaving(false);
    }
  }, [activeSlideId, artifactId, data, refresh]);

  const submitHistory = useCallback((action: "undo" | "redo") => {
    if (!availableCapabilities.has("presentation.history") || !data || saving) return;
    const enabled = action === "undo" ? data.history.canUndo : data.history.canRedo;
    if (!enabled) return;
    const transaction = presentationHistoryTransaction(action);
    void submit(transaction.commands, transaction.origin);
  }, [availableCapabilities, data, saving, submit]);

  const updatePresenceCursor = useCallback((cursor: { x: number; y: number }) => {
    const previous = presenceCursor.current;
    if (previous && Math.abs(previous.x - cursor.x) < 4 && Math.abs(previous.y - cursor.y) < 4) {
      return;
    }
    presenceCursor.current = cursor;
    publishPresence();
  }, [publishPresence]);

  return {
    data,
    activeSlideId,
    error,
    saving,
    availableCapabilities,
    capabilitiesLoaded,
    thumbnailDirtyIds,
    remotePresence,
    refresh,
    submit,
    submitHistory,
    setError,
    setSaving,
    updatePresenceCursor,
  };
}

function createPresenceSessionId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `presentation-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
