import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type Settings = {
  sensitivity: number;
  cooldownMs: number;
  detectionEnabled: boolean;
  slapCount: number;
  sensorConnected: boolean;
  usesCustomSound: boolean;
  maxSlapSoundSeconds: number;
  /** macOS system output level (0–100) only while slap audio plays */
  slapVolumePercent: number;
  /** Open at login (after you sign in; not before the login screen). */
  launchAtLogin: boolean;
};

type ImportSoundResponse = {
  settings: Settings;
  import: {
    durationSeconds: number;
    wasTrimmed: boolean;
    maxSeconds: number;
  };
};

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [lastStrength, setLastStrength] = useState<number | null>(null);
  const [soundNotice, setSoundNotice] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback(async () => {
    try {
      const s = await invoke<Settings>("get_settings");
      setSettings(s);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<{ totalCount: number; strength: number }>("slap", (e) => {
      setLastStrength(e.payload.strength);
      setSettings((prev) =>
        prev ? { ...prev, slapCount: e.payload.totalCount } : prev,
      );
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const pushSettings = useCallback(
    async (next: Settings) => {
      try {
        const s = await invoke<Settings>("set_settings", {
          payload: {
            sensitivity: next.sensitivity,
            cooldownMs: next.cooldownMs,
            detectionEnabled: next.detectionEnabled,
            slapVolumePercent: Math.round(
              Math.min(100, Math.max(0, next.slapVolumePercent)),
            ),
            launchAtLogin: next.launchAtLogin,
          },
        });
        setSettings(s);
        setLoadError(null);
      } catch (e) {
        setLoadError(String(e));
      }
    },
    [],
  );

  const scheduleSave = useCallback(
    (next: Settings) => {
      setSettings(next);
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => {
        void pushSettings(next);
      }, 320);
    },
    [pushSettings],
  );

  const onTestSlap = async () => {
    try {
      const s = await invoke<Settings>("test_slap");
      setSettings(s);
      setLastStrength(1);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  };

  const onReset = async () => {
    try {
      const s = await invoke<Settings>("reset_counter");
      setSettings(s);
      setLastStrength(null);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  };

  const onPickSound = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Audio",
            extensions: [
              "wav",
              "mp3",
              "flac",
              "ogg",
              "oga",
              "m4a",
              "m4b",
              "mp4",
              "mov",
              "aac",
              "aiff",
              "aif",
            ],
          },
        ],
      });
      if (selected == null) return;
      const path = typeof selected === "string" ? selected : selected[0];
      const res = await invoke<ImportSoundResponse>("import_custom_sound", {
        path,
      });
      setSettings(res.settings);
      const { durationSeconds, wasTrimmed, maxSeconds } = res.import;
      setLoadError(null);
      const secs =
        typeof durationSeconds === "number" && Number.isFinite(durationSeconds)
          ? durationSeconds.toFixed(2)
          : "?";
      const trimNote = wasTrimmed
        ? ` Trimmed to first ${maxSeconds}s.`
        : "";
      setSoundNotice(`Using your file (~${secs}s).${trimNote}`);
    } catch (e) {
      setSoundNotice(null);
      setLoadError(String(e));
    }
  };

  const onClearSound = async () => {
    try {
      const s = await invoke<Settings>("clear_custom_sound");
      setSettings(s);
      setSoundNotice(null);
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  };

  if (!settings) {
    return (
      <main className="shell">
        <p className="muted">
          {loadError ?? "Loading…"}
        </p>
      </main>
    );
  }

  return (
    <main className="shell">
      <header className="hero">
        <p className="eyebrow">Menu bar · Apple Silicon</p>
        <h1>Slap My Mac</h1>
        <p className="lede">
          Tap beside the trackpad; when the motion sensor sees a sharp jolt, you
          get a reaction sound. Tune sensitivity if it is too quiet or too
          jumpy.
        </p>
      </header>

      <section className="card stats">
        <div>
          <span className="stat-label">Slaps</span>
          <span className="stat-value">{settings.slapCount}</span>
        </div>
        <div>
          <span className="stat-label">Sensor</span>
          <span
            className={
              settings.sensorConnected ? "stat-ok" : "stat-warn"
            }
          >
            {settings.sensorConnected ? "Connected" : "Not connected"}
          </span>
        </div>
        {lastStrength != null && (
          <div className="strength">
            Last hit strength: {lastStrength.toFixed(2)}×
          </div>
        )}
      </section>

      {!settings.sensorConnected && (
        <p className="hint">
          If this stays disconnected, the app could not open the internal IMU
          (permissions, desktop Mac, or an unsupported machine). The app wakes
          Apple&apos;s SPU drivers on startup; wait a few seconds and try again.
          You can still use <strong>Test slap</strong> from the menu bar.
        </p>
      )}

      {loadError && <p className="error">{loadError}</p>}
      {soundNotice && <p className="sound-notice">{soundNotice}</p>}

      <section className="card sound-card">
        <span className="field-label">Slap sound</span>
        <p className="sound-desc">
          Built-in clip, or choose a file (max{" "}
          <strong>{settings.maxSlapSoundSeconds}s</strong> kept; longer files
          are trimmed).
        </p>
        <div className="sound-row">
          <span className="sound-status">
            {settings.usesCustomSound ? "Using your file" : "Using built-in"}
          </span>
          <div className="sound-actions">
            <button
              type="button"
              className="btn ghost btn-compact"
              onClick={() => void onPickSound()}
            >
              Choose file…
            </button>
            {settings.usesCustomSound && (
              <button
                type="button"
                className="btn ghost btn-compact"
                onClick={() => void onClearSound()}
              >
                Use built-in
              </button>
            )}
          </div>
        </div>
      </section>

      <section className="card">
        <span className="field-label">Slap volume (system)</span>
        <p className="sound-desc">
          While a slap plays, macOS output is set to this level, then restored to
          what it was before. Overlapping slaps keep that until the last one
          finishes.
        </p>
        <label className="field">
          <span className="field-label">
            Output level{" "}
            <span className="field-hint">{settings.slapVolumePercent}%</span>
          </span>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={settings.slapVolumePercent}
            onChange={(e) =>
              scheduleSave({
                ...settings,
                slapVolumePercent: Number(e.target.value),
              })
            }
          />
        </label>
      </section>

      <section className="card controls">
        <label className="field">
          <span className="field-label">
            Sensitivity{" "}
            <span className="field-hint">
              (1 = hard slaps only, 10 = light taps too) ·{" "}
              {settings.sensitivity.toFixed(1)}
            </span>
          </span>
          <input
            type="range"
            min={1}
            max={10}
            step={0.5}
            value={settings.sensitivity}
            onChange={(e) =>
              scheduleSave({
                ...settings,
                sensitivity: Number(e.target.value),
              })
            }
          />
        </label>

        <label className="field">
          <span className="field-label">
            Cooldown (ms) <span className="field-hint">{settings.cooldownMs}</span>
          </span>
          <input
            type="range"
            min={80}
            max={3000}
            step={10}
            value={settings.cooldownMs}
            onChange={(e) =>
              scheduleSave({
                ...settings,
                cooldownMs: Number(e.target.value),
              })
            }
          />
        </label>

        <label className="toggle">
          <input
            type="checkbox"
            checked={settings.detectionEnabled}
            onChange={(e) =>
              void pushSettings({
                ...settings,
                detectionEnabled: e.target.checked,
              })
            }
          />
          <span>Listen for real slaps</span>
        </label>

        <label className="toggle">
          <input
            type="checkbox"
            checked={settings.launchAtLogin}
            onChange={(e) =>
              void pushSettings({
                ...settings,
                launchAtLogin: e.target.checked,
              })
            }
          />
          <span>Open at login</span>
        </label>
        <p className="sound-desc login-items-hint">
          Starts with your user session after restart or power on. You may see a
          macOS prompt to allow this in{" "}
          <strong>System Settings → General → Login Items</strong>.
        </p>
      </section>

      <div className="actions">
        <button type="button" className="btn primary" onClick={() => void onTestSlap()}>
          Test slap
        </button>
        <button type="button" className="btn ghost" onClick={() => void onReset()}>
          Reset counter
        </button>
      </div>

      <footer className="foot">
        <p>
          Tip: click the menu bar icon for quick actions. Close this window to
          hide the app (it keeps running).
        </p>
      </footer>
    </main>
  );
}
