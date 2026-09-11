import { useCallback, useState } from "react";

import { presenterAudienceUrl } from "./presenter-session.js";

export type PresentationLaunch =
  | { readonly mode: "single"; readonly popupBlocked: boolean }
  | { readonly mode: "presenter"; readonly sessionId: string; readonly audienceWindow: Window };

export function usePresentationLauncher(artifactId: string) {
  const [launch, setLaunch] = useState<PresentationLaunch | null>(null);
  const startSingle = useCallback(() => setLaunch({ mode: "single", popupBlocked: false }), []);
  const startPresenter = useCallback(() => {
    const sessionId = createSessionId();
    const audienceWindow = window.open(
      presenterAudienceUrl(window.location.href, artifactId, sessionId),
      `open-office-presentation-${artifactId}`,
      "popup=yes",
    );
    setLaunch(audienceWindow
      ? { mode: "presenter", sessionId, audienceWindow }
      : { mode: "single", popupBlocked: true });
  }, [artifactId]);
  const exit = useCallback(() => {
    setLaunch((current) => {
      if (current?.mode === "presenter" && !current.audienceWindow.closed) current.audienceWindow.close();
      return null;
    });
  }, []);
  return { launch, startSingle, startPresenter, exit } as const;
}

function createSessionId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
