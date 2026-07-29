import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const ENABLED_KEY = "pluxel.updateCheck";
const LAST_KEY = "pluxel.updateLastCheck";
const CACHE_MS = 6 * 60 * 60 * 1000; // Re-check on startup at most every 6 hours.

/** Whether the automatic startup update check is enabled (default: on). */
export function updateCheckEnabled(): boolean {
  return localStorage.getItem(ENABLED_KEY) !== "0";
}

export type UpdatePhase =
  | "idle" // nothing to show
  | "checking" // querying the endpoint
  | "available" // a newer version was found
  | "downloading" // applying the update
  | "uptodate" // manual check found nothing new
  | "error"; // manual check failed

/**
 * Drives the Tauri auto-updater: a silent startup check (throttled, honouring
 * the user's off-switch) plus a manual "check now" path. Surfaces state for an
 * update prompt; downloading applies the update in place and relaunches.
 */
export function useUpdater() {
  const [enabled, setEnabledState] = useState(updateCheckEnabled);
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);

  const setEnabled = useCallback((on: boolean) => {
    localStorage.setItem(ENABLED_KEY, on ? "1" : "0");
    setEnabledState(on);
  }, []);

  const runCheck = useCallback(async (manual: boolean) => {
    if (busy.current) return;
    busy.current = true;
    setError(null);
    if (manual) setPhase("checking");
    try {
      const u = await check();
      localStorage.setItem(LAST_KEY, String(Date.now()));
      if (u) {
        setUpdate(u);
        setPhase("available");
      } else {
        // Only tell the user "up to date" when they asked explicitly.
        setPhase(manual ? "uptodate" : "idle");
      }
    } catch (e) {
      // check() throws in dev / offline builds — stay silent unless manual.
      setError(String(e));
      setPhase(manual ? "error" : "idle");
    } finally {
      busy.current = false;
    }
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setPhase("downloading");
    setProgress(0);
    try {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((e) => {
        switch (e.event) {
          case "Started":
            total = e.data.contentLength ?? 0;
            break;
          case "Progress":
            done += e.data.chunkLength;
            if (total) setProgress(done / total);
            break;
          case "Finished":
            setProgress(1);
            break;
        }
      });
      await relaunch();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }, [update]);

  const dismiss = useCallback(() => {
    setPhase("idle");
    setUpdate(null);
    setError(null);
  }, []);

  // One silent startup check, throttled to the cache window and off-switch.
  useEffect(() => {
    if (!enabled) return;
    const last = Number(localStorage.getItem(LAST_KEY) || 0);
    if (Date.now() - last < CACHE_MS) return;
    void runCheck(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    enabled,
    setEnabled,
    phase,
    update,
    progress,
    error,
    checkNow: () => runCheck(true),
    install,
    dismiss,
  };
}
