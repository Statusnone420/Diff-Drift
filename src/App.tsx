import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Flag, SessionData } from "./types";
import { TitleBar } from "./components/TitleBar";
import { Toolbar } from "./components/Toolbar";
import { Sidebar } from "./components/Sidebar";
import { Center } from "./components/Center";
import { RightPanel } from "./components/RightPanel";
import { EmptyState } from "./components/EmptyState";
import { onMaximizeChange } from "./lib/window";
import {
  dismissAll,
  exportReport,
  initSession,
  onDrift,
  openRepo,
  pickFolder,
  setApproved,
  setBaseline,
  setFlagDismissed,
  setNodeReviewed,
} from "./lib/session";

function hhmm(): string {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

type Status = "init" | "onboarding" | "loading" | "loaded";

export default function App() {
  const [status, setStatus] = useState<Status>("init");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [data, setData] = useState<SessionData | null>(null);
  const [watchingSince, setWatchingSince] = useState<string | null>(null);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const [activeFlagId, setActiveFlagId] = useState<string | null>(null);
  const [pulseId, setPulseId] = useState<string | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [justUpdated, setJustUpdated] = useState(false);

  const nodeRefs = useRef<Record<string, HTMLDivElement>>({});
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pulseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const updatedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flagsById = useMemo(() => {
    const m: Record<string, Flag> = {};
    data?.flags.forEach((f) => (m[f.id] = f));
    return m;
  }, [data]);

  const registerRef = useCallback((id: string, el: HTMLDivElement | null) => {
    if (el) nodeRefs.current[id] = el;
  }, []);

  const scrollToNode = useCallback((nodeId: string) => {
    requestAnimationFrame(() => {
      const el = nodeRefs.current[nodeId];
      const cont = scrollRef.current;
      if (el && cont) {
        const r = el.getBoundingClientRect();
        const cr = cont.getBoundingClientRect();
        const target = cont.scrollTop + (r.top - cr.top) - 96;
        cont.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
      }
    });
  }, []);

  const firePulse = useCallback((nodeId: string) => {
    setPulseId(null);
    if (pulseTimer.current) clearTimeout(pulseTimer.current);
    requestAnimationFrame(() => {
      setPulseId(nodeId);
      pulseTimer.current = setTimeout(() => setPulseId(null), 720);
    });
  }, []);

  /** Transient, non-blocking message (e.g. a failed repo switch while a session is live). */
  const showNotice = useCallback((message: string) => {
    setNotice(message);
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), 6000);
  }, []);

  // First load of a repo: set the default selection (highest-severity active flag) + scroll.
  const receiveInitial = useCallback(
    (d: SessionData) => {
      nodeRefs.current = {};
      setData(d);
      setError(null);
      setNotice(null);
      setWatchingSince(hhmm());
      setStatus("loaded");
      const f0 = d.flags.find((f) => !f.dismissed);
      if (f0) {
        setSelectedId(f0.fileId);
        setActiveNodeId(f0.nodeId);
        setActiveFlagId(f0.id);
        setTimeout(() => scrollToNode(f0.nodeId), 120);
      } else {
        setSelectedId(d.files[0]?.id ?? null);
        setActiveNodeId(null);
        setActiveFlagId(null);
      }
    },
    [scrollToNode]
  );

  const openPath = useCallback(
    async (path: string) => {
      // With a session already on screen, keep it visible while the new repo loads —
      // and keep it if opening fails (a bad pick shouldn't destroy a live session).
      const hadSession = data !== null;
      if (!hadSession) {
        setStatus("loading");
        setError(null);
      }
      try {
        receiveInitial(await openRepo(path));
      } catch (e) {
        const message = typeof e === "string" ? e : String(e);
        if (hadSession) {
          showNotice(message);
        } else {
          setStatus("onboarding");
          setError(message);
        }
      }
    },
    [data, receiveInitial, showNotice]
  );

  const pickAndOpen = useCallback(async () => {
    try {
      const p = await pickFolder();
      if (p) await openPath(p);
    } catch (e) {
      setError(String(e));
    }
  }, [openPath]);

  // Mount: restore the last repo (or onboarding) + subscribe to live updates.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    initSession()
      .then((d) => {
        if (cancelled) return;
        if (d) receiveInitial(d);
        else setStatus("onboarding");
      })
      .catch(() => {
        if (!cancelled) setStatus("onboarding");
      });
    // Live re-analysis: replace data in place; selection is resolved against the new
    // data at render with fallback, so it survives if the node/flag still exists.
    onDrift((next) => {
      if (cancelled) return;
      setData(next);
      setJustUpdated(true);
      if (updatedTimer.current) clearTimeout(updatedTimer.current);
      updatedTimer.current = setTimeout(() => setJustUpdated(false), 3000);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((e) => {
        console.error("Drift listener failed to attach:", e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [receiveInitial]);

  useEffect(() => onMaximizeChange(setMaximized), []);

  const selectFlag = useCallback(
    (flagId: string) => {
      const fl = flagsById[flagId];
      if (!fl) return;
      setActiveFlagId(flagId);
      setActiveNodeId(fl.nodeId);
      if (fl.fileId !== selectedId) {
        setSelectedId(fl.fileId);
        setTimeout(() => {
          scrollToNode(fl.nodeId);
          firePulse(fl.nodeId);
        }, 90);
      } else {
        scrollToNode(fl.nodeId);
        firePulse(fl.nodeId);
      }
    },
    [flagsById, selectedId, scrollToNode, firePulse]
  );

  const toggleFlagFromNode = useCallback((flagId: string) => selectFlag(flagId), [selectFlag]);

  const selectFile = useCallback((fileId: string) => {
    setSelectedId(fileId);
    setActiveNodeId(null);
    setActiveFlagId(null);
  }, []);

  // ---------- triage / approval / export ----------
  const applyTriage = useCallback(
    (next: SessionData, clearSelection?: boolean) => {
      setData(next);
      if (clearSelection) {
        setActiveFlagId(null);
        setActiveNodeId(null);
      }
    },
    []
  );

  const handleDismissFlag = useCallback(
    async (flagId: string, dismissed: boolean) => {
      try {
        applyTriage(await setFlagDismissed(flagId, dismissed), dismissed && flagId === activeFlagId);
      } catch (e) {
        showNotice(String(e));
      }
    },
    [applyTriage, activeFlagId, showNotice]
  );

  const handleDismissAll = useCallback(async () => {
    try {
      applyTriage(await dismissAll(), true);
    } catch (e) {
      showNotice(String(e));
    }
  }, [applyTriage, showNotice]);

  const handleToggleReviewed = useCallback(
    async (nodeId: string, reviewed: boolean) => {
      try {
        applyTriage(await setNodeReviewed(nodeId, reviewed));
      } catch (e) {
        showNotice(String(e));
      }
    },
    [applyTriage, showNotice]
  );

  const handleSetBaseline = useCallback(
    async (spec: string) => {
      try {
        // New baseline → new node ids; clear the selection rather than point at ghosts.
        applyTriage(await setBaseline(spec), true);
      } catch (e) {
        showNotice(String(e));
      }
    },
    [applyTriage, showNotice]
  );

  const handleToggleApprove = useCallback(async () => {
    if (!data) return;
    try {
      const next = !data.session.approved;
      applyTriage(await setApproved(next, next ? hhmm() : null));
    } catch (e) {
      showNotice(String(e));
    }
  }, [data, applyTriage, showNotice]);

  const handleExport = useCallback(async (): Promise<boolean> => {
    if (!data) return false;
    try {
      const path = await exportReport(data.session.project);
      return path !== null; // null = user cancelled the save dialog
    } catch (e) {
      showNotice(String(e));
      return false;
    }
  }, [data, showNotice]);

  const shell = (children: ReactNode) => (
    <div className={"window" + (maximized ? " maximized" : "")}>
      <TitleBar maximized={maximized} />
      {children}
    </div>
  );

  if (status !== "loaded" || !data) {
    return shell(
      status === "loading" ? (
        <div className="app-loading">Analyzing working tree…</div>
      ) : (
        <EmptyState error={error} onOpen={pickAndOpen} />
      )
    );
  }

  const { session, files, flags } = data;
  // Resolve selection with graceful fallback (ids may vanish after a live update).
  const file = files.find((f) => f.id === selectedId) ?? files[0] ?? null;
  const activeFlag = flags.find((f) => f.id === activeFlagId) ?? null;

  return shell(
    <>
      <Toolbar
        session={session}
        onSwitchRepo={pickAndOpen}
        onDismissAll={handleDismissAll}
        onToggleApprove={handleToggleApprove}
        onSetBaseline={handleSetBaseline}
      />
      {notice && (
        <div className="app-notice" role="alert">
          {notice}
          <button className="notice-close" aria-label="Dismiss message" onClick={() => setNotice(null)}>
            ×
          </button>
        </div>
      )}
      <div className="body">
        <Sidebar
          session={session}
          files={files}
          selectedId={file?.id ?? null}
          onSelect={selectFile}
          watchingSince={watchingSince}
          justUpdated={justUpdated}
        />
        <Center
          file={file}
          changedFiles={session.changedFiles}
          flagsById={flagsById}
          activeNodeId={activeNodeId}
          pulseId={pulseId}
          onToggleFlag={toggleFlagFromNode}
          onToggleReviewed={handleToggleReviewed}
          registerRef={registerRef}
          scrollRef={scrollRef}
        />
        <RightPanel
          flags={flags}
          activeFlagId={activeFlag?.id ?? null}
          onSelectFlag={selectFlag}
          onDismissFlag={handleDismissFlag}
          onExport={handleExport}
        />
      </div>
    </>
  );
}
