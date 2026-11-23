import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "./api";

/** One app in the list, as edited (processes stay a comma-separated string). */
interface Row {
  id: number;
  key: string;
  processes: string;
  launch: string;
  /** Name and icon; undefined while loading. */
  look?: api.AppLook;
  /** Process/launch fields shown. */
  details?: boolean;
}

type Message = { text: string; kind: "ok" | "error" };

/** How long the "Saved" confirmation stays after the bar would otherwise hide. */
const SAVED_MS = 3000;

/** Everything saved to the config, as one comparable string. */
const snapshot = (modifier: string, log: boolean, rows: Row[]) =>
  JSON.stringify([modifier, log, rows.map((r) => [r.key, r.processes, r.launch])]);

let nextId = 1;
const toRow = (a: api.AppSettings, extra: Partial<Row> = {}): Row => ({
  id: nextId++,
  key: a.key,
  processes: a.processes.join(", "),
  launch: a.launch,
  ...extra,
});
const toApp = (r: Row): api.AppSettings => ({
  key: r.key.trim(),
  processes: r.processes.split(",").map((s) => s.trim()).filter(Boolean),
  launch: r.launch.trim(),
});

/** The picker is open to add an app, or to change row `id`'s app. */
type Picking = { mode: "add" } | { mode: "change"; id: number };

export default function App() {
  const [path, setPath] = useState("");
  const [broken, setBroken] = useState<string | null>(null);
  const [modifier, setModifier] = useState("");
  const [log, setLog] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);
  const [rord, setRord] = useState<api.Status | null | undefined>(undefined);
  /** Problems with the config file on disk, shown at the top. */
  const [diskError, setDiskError] = useState<string | null>(null);
  /** Result of the last save, or an error, shown in the bottom bar. */
  const [message, setMessage] = useState<Message | null>(null);
  /** Snapshot of the form as last loaded or saved; differs => unsaved changes. */
  const [baseline, setBaseline] = useState<string | null>(null);
  const [picking, setPicking] = useState<Picking | null>(null);
  const savedTimer = useRef<number | undefined>(undefined);

  const dirty = baseline !== null && snapshot(modifier, log, rows) !== baseline;

  const refreshStatus = useCallback(async () => setRord(await api.status()), []);

  /** Fetch names and icons for the given rows. */
  const describe = useCallback(async (targets: Row[]) => {
    if (targets.length === 0) return;
    // If names and icons can't be fetched, show letter tiles rather than
    // placeholders that never fill in.
    const looks: api.AppLook[] = await api.describeApps(targets.map(toApp)).catch(() =>
      targets.map((r) => ({ name: r.processes.split(",")[0]?.trim() ?? "", icon: null })),
    );
    const byId = new Map(targets.map((r, i) => [r.id, looks[i]]));
    setRows((rs) => rs.map((r) => (byId.has(r.id) ? { ...r, look: byId.get(r.id) } : r)));
  }, []);

  const reload = useCallback(async () => {
    try {
      const r = await api.load();
      setPath(r.path);
      setMessage(null);
      if (r.settings) {
        const loaded = r.settings.apps.map((a) => toRow(a));
        setBroken(null);
        setModifier(r.settings.modifier);
        setLog(r.settings.log);
        setRows(loaded);
        setBaseline(snapshot(r.settings.modifier, r.settings.log, loaded));
        setDiskError(r.error);
        void describe(loaded);
      } else {
        setDiskError(null);
        setBroken(r.error ?? "unknown error");
      }
      await refreshStatus();
    } catch (e) {
      setMessage({ text: String(e), kind: "error" });
    }
  }, [refreshStatus, describe]);

  const save = useCallback(async () => {
    if (!dirty) return;
    try {
      const text = await api.save({ modifier: modifier.trim(), log, apps: rows.map(toApp) });
      setBaseline(snapshot(modifier, log, rows));
      setDiskError(null);
      await refreshStatus();
      setMessage({ text, kind: "ok" });
      window.clearTimeout(savedTimer.current);
      savedTimer.current = window.setTimeout(() => setMessage(null), SAVED_MS);
    } catch (e) {
      setMessage({ text: String(e), kind: "error" });
    }
  }, [dirty, modifier, log, rows, refreshStatus]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Ctrl+S saves.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "s") {
        e.preventDefault();
        void save();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [save]);

  // Any edit makes the last save result or error stale.
  const edited = () => setMessage(null);
  const updateRow = (id: number, patch: Partial<Row>) => {
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, ...patch } : r)));
    edited();
  };
  const moveRow = (i: number, by: -1 | 1) => {
    setRows((rs) => {
      const next = [...rs];
      [next[i], next[i + by]] = [next[i + by], next[i]];
      return next;
    });
    edited();
  };

  const pick = (app: api.RunningApp) => {
    const look = { name: app.name, icon: app.icon };
    if (picking?.mode === "change") {
      updateRow(picking.id, { processes: app.process, launch: app.launch, look });
    } else {
      const key = suggestKey(app.name, app.process, rows);
      setRows((rs) => [...rs, toRow({ key, processes: [app.process], launch: app.launch }, { look })]);
      edited();
    }
    setPicking(null);
  };

  // Keys used by more than one row, and rord's per-key status.
  const counts = new Map<string, number>();
  for (const r of rows) {
    const k = r.key.trim().toLowerCase();
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  const keyStatus = new Map(rord?.hotkeys.map((h) => [h.key, h.error]) ?? []);

  return (
    <>
      <main>
        <header>
          <h1>ror settings</h1>
          <span className={`pill ${rord === undefined ? "" : rord ? "on" : "off"}`}>
            {rord === undefined ? "checking…" : rord ? "running" : "not running"}
          </span>
          <span className="path">{path}</span>
        </header>

        {diskError && (
          <section className="problem">
            The config file on disk has problems (ror is using the last good config):{"\n\n"}
            {diskError}
          </section>
        )}

        {broken !== null ? (
          <section className="problem">
            config.toml can't be read, fix it in a text editor (ror edit):{"\n\n"}
            {broken}
          </section>
        ) : (
          <>
            <section>
              <div className="section-head">
                <h2>Apps</h2>
                <span className="hint">
                  Hold <kbd>{modifier || "ctrl+shift+alt"}</kbd> and press the key to switch to the app.
                </span>
              </div>
              <ul className="apps">
                {rows.map((r, i) => (
                  <AppRow
                    key={r.id}
                    row={r}
                    duplicate={(counts.get(r.key.trim().toLowerCase()) ?? 0) > 1}
                    keyError={keyStatus.get(r.key.trim().toLowerCase()) ?? null}
                    onChange={(patch) => updateRow(r.id, patch)}
                    onDetailsDone={() => void describe([r])}
                    onChangeApp={() => setPicking({ mode: "change", id: r.id })}
                    onMoveUp={i > 0 ? () => moveRow(i, -1) : undefined}
                    onMoveDown={i < rows.length - 1 ? () => moveRow(i, 1) : undefined}
                    onRemove={() => {
                      setRows((rs) => rs.filter((x) => x.id !== r.id));
                      edited();
                    }}
                  />
                ))}
                {rows.length === 0 && <li className="empty">No apps yet.</li>}
              </ul>
              <button className="primary add" onClick={() => setPicking({ mode: "add" })}>
                + Add app
              </button>
            </section>

            <section>
              <h2>Settings</h2>
              <div className="row">
                <label>
                  Modifier
                  <input
                    id="modifier"
                    type="text"
                    spellCheck={false}
                    value={modifier}
                    onChange={(e) => {
                      setModifier(e.target.value);
                      edited();
                    }}
                  />
                </label>
                <span className="hint">ctrl, shift, alt, win joined with "+"</span>
                <label>
                  <input
                    type="checkbox"
                    checked={log}
                    onChange={(e) => {
                      setLog(e.target.checked);
                      edited();
                    }}
                  />
                  Debug log
                </label>
              </div>
            </section>
          </>
        )}
      </main>

      {picking && (
        <AppPicker
          rows={rows}
          changing={picking.mode === "change" ? picking.id : undefined}
          onPick={pick}
          onManual={() => {
            setRows((rs) => [...rs, toRow({ key: "", processes: [], launch: "" }, { details: true })]);
            setPicking(null);
            edited();
          }}
          onClose={() => setPicking(null)}
        />
      )}

      {(dirty || message) && (
        <footer>
          <div className="inner">
            <span id="message" className={message?.kind ?? ""}>
              {message?.text ?? "Unsaved changes"}
            </span>
            {dirty && (
              <>
                <button onClick={() => void reload()}>Revert</button>
                <button className="primary" onClick={() => void save()}>
                  Save
                </button>
              </>
            )}
          </div>
        </footer>
      )}
    </>
  );
}

/** An app's icon, or a letter tile if it has none. */
function AppIcon({ look, fallback, size }: { look?: api.AppLook; fallback: string; size: number }) {
  if (look?.icon) return <img className="app-icon" src={look.icon} width={size} height={size} alt="" />;
  const letter = (look?.name || fallback || "?").trim().charAt(0).toUpperCase() || "?";
  return (
    <span className="app-icon tile" style={{ width: size, height: size, fontSize: size * 0.5 }}>
      {look === undefined && fallback ? "" : letter}
    </span>
  );
}

function AppRow(props: {
  row: Row;
  duplicate: boolean;
  /** Why rord couldn't register this key, if it couldn't. */
  keyError: string | null;
  onChange: (patch: Partial<Row>) => void;
  onDetailsDone: () => void;
  onChangeApp: () => void;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  onRemove: () => void;
}) {
  const { row, duplicate, keyError, onChange } = props;
  const process = row.processes.split(",")[0]?.trim() ?? "";
  const keyProblem = duplicate ? "Used by more than one app" : keyError ? `Not active: ${keyError}` : undefined;

  return (
    <li className="app">
      <div className="app-main">
        <button className="app-pick" onClick={props.onChangeApp} title="Change app">
          <AppIcon look={row.look} fallback={process} size={32} />
          <span className="app-text">
            <span className="app-name">{process ? row.look?.name ?? process : "Choose an app…"}</span>
            {process && <span className="app-sub">{row.processes}</span>}
          </span>
        </button>
        <input
          className={`keycap ${duplicate ? "bad" : keyError ? "warn" : ""}`}
          type="text"
          spellCheck={false}
          maxLength={3}
          placeholder="?"
          aria-label="Key"
          title={keyProblem ?? "The key to press"}
          value={row.key}
          onChange={(e) => onChange({ key: e.target.value.toLowerCase() })}
        />
        <RowMenu
          items={[
            { label: "Change app…", onSelect: props.onChangeApp },
            { label: row.details ? "Hide details" : "Edit details", onSelect: () => onChange({ details: !row.details }) },
            { label: "Move up", onSelect: props.onMoveUp },
            { label: "Move down", onSelect: props.onMoveDown },
            { label: "Remove", onSelect: props.onRemove, danger: true },
          ]}
        />
      </div>
      {row.details && (
        <div className="app-details">
          <label>
            Process name(s)
            <input
              type="text"
              spellCheck={false}
              placeholder="firefox.exe"
              value={row.processes}
              onChange={(e) => onChange({ processes: e.target.value })}
              onBlur={props.onDetailsDone}
            />
          </label>
          <label>
            Launch command
            <input
              type="text"
              spellCheck={false}
              placeholder="(the process name)"
              value={row.launch}
              onChange={(e) => onChange({ launch: e.target.value })}
              onBlur={props.onDetailsDone}
            />
          </label>
          <p className="hint">
            Process name: the program that owns the app's windows (Task Manager → Details); separate
            several with commas. Launch command: what runs when the app isn't open; empty runs the
            process name. %VARIABLES% work.
          </p>
        </div>
      )}
    </li>
  );
}

/** A free key for an app: a letter from its name if possible, else any free key. */
function suggestKey(name: string, process: string, rows: Row[]): string {
  const used = new Set(rows.map((r) => r.key.trim().toLowerCase()));
  const candidates = [...name.toLowerCase(), ...process.toLowerCase(), ..."abcdefghijklmnopqrstuvwxyz0123456789"];
  return candidates.find((c) => /[a-z0-9]/.test(c) && !used.has(c)) ?? "";
}

/** Choose an app from the ones with open windows. */
function AppPicker(props: {
  rows: Row[];
  /** Row whose app is being changed; undefined when adding. */
  changing?: number;
  onPick: (app: api.RunningApp) => void;
  onManual: () => void;
  onClose: () => void;
}) {
  const { rows, changing, onPick, onManual, onClose } = props;
  const [apps, setApps] = useState<api.RunningApp[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const refresh = useCallback(() => {
    setApps(null);
    setError(null);
    api.runningApps().then(setApps, (e) => setError(String(e)));
  }, []);
  useEffect(refresh, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  /** The row (if any) that already has this process. */
  const owner = (process: string) =>
    rows.find((r) => r.processes.split(",").some((p) => p.trim().toLowerCase() === process.toLowerCase()));

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (apps ?? []).filter(
      (a) => !q || [a.name, a.process, a.title].some((s) => s.toLowerCase().includes(q)),
    );
  }, [apps, query]);

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog" role="dialog" aria-label="Choose an app">
        <div className="dialog-head">
          <h2>{changing === undefined ? "Add an app" : "Change app"}</h2>
          <button className="icon" title="Refresh" onClick={refresh}>
            ↻
          </button>
          <button className="icon" title="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <input
          className="search"
          type="search"
          placeholder="Search open apps"
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            const first = shown.find((a) => {
              const o = owner(a.process);
              return !o || o.id === changing;
            });
            if (e.key === "Enter" && first) onPick(first);
          }}
        />
        {error && <p className="problem">{error}</p>}
        {apps === null && !error && <p className="hint">Looking for open apps…</p>}
        {apps !== null && shown.length === 0 && (
          <p className="hint">{query ? "No open app matches." : "No apps with open windows found."}</p>
        )}
        <ul className="picker">
          {shown.map((a) => {
            const o = owner(a.process);
            const current = o !== undefined && o.id === changing;
            const taken = o !== undefined && !current;
            return (
              <li key={a.process}>
                <button disabled={taken} onClick={() => onPick(a)}>
                  <AppIcon look={a} fallback={a.process} size={32} />
                  <span className="app-text">
                    <span className="app-name">{a.name}</span>
                    <span className="app-sub">{a.title || a.process}</span>
                  </span>
                  {o && (
                    <span className="badge">
                      {current ? "current" : <>on <kbd>{o.key || "?"}</kbd></>}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
        <p className="hint dialog-foot">
          Not listed? Open the app, then refresh ↻. Or{" "}
          <button className="link" onClick={onManual}>
            enter the details by hand
          </button>
          .
        </p>
      </div>
    </div>
  );
}

/** A ⋯ button with a small dropdown. Items without `onSelect` are disabled. */
function RowMenu({ items }: { items: { label: string; onSelect?: () => void; danger?: boolean }[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close on a click outside or Escape.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="row-menu" ref={ref}>
      <button
        className="icon"
        title="More"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        ⋯
      </button>
      {open && (
        <div className="row-menu-items" role="menu">
          {items.map((item) => (
            <button
              key={item.label}
              role="menuitem"
              className={item.danger ? "danger" : undefined}
              disabled={!item.onSelect}
              onClick={() => {
                setOpen(false);
                item.onSelect?.();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
