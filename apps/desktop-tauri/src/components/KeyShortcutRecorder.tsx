import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Phase 8 KeyShortcutRecorder port. Captures a global-hotkey chord
// the user types while focused, validates it via `hotkey_test_chord`,
// and commits it via `hotkey_set_chord`. Mirrors the macOS
// `KeyShortcutRecorder` widget so users can rebind Win+Shift+U to
// anything else without editing config files.
//
// Workflow:
//   1. User clicks the field → state becomes "recording", aria-pressed.
//   2. Next keydown with at least one modifier captures the chord.
//   3. Chord is dry-run via `hotkey_test_chord`; on success the widget
//      shows the new chord in normal state and invokes `onChange`.
//   4. Escape cancels recording without changing the binding.
//   5. Clear button restores the platform default ("Win+Shift+U").
//
// The actual registration (unregister-old, register-new) is the
// parent's responsibility — typically `onChange` calls
// `hotkey_set_chord` and persists the chord into Settings.

interface Props {
  /** Current chord, e.g. "Win+Shift+U". Empty string falls back to default label. */
  value: string;
  /** Called with the new chord string after the user records one. */
  onChange: (chord: string) => void;
  /** Called when the user clears the binding to the platform default. */
  onClear: () => void;
  /** Label for the field; defaults to "Hotkey". */
  label?: string;
  /** Disable interaction entirely. */
  disabled?: boolean;
}

const MODIFIER_LABELS: Record<string, string> = {
  Control: "Ctrl",
  Shift: "Shift",
  Alt: "Alt",
  Meta: "Win",
};

function eventToChord(event: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (event.ctrlKey) mods.push("Ctrl");
  if (event.shiftKey) mods.push("Shift");
  if (event.altKey) mods.push("Alt");
  if (event.metaKey) mods.push("Win");
  if (mods.length === 0) return null;

  let key: string | null = null;
  const k = event.key;
  // Letter / digit
  if (k.length === 1 && /^[a-zA-Z0-9]$/.test(k)) {
    key = k.toUpperCase();
  } else if (/^F([1-9]|1[0-2])$/.test(k)) {
    key = k.toUpperCase();
  }
  if (!key) return null;
  return [...mods, key].join("+");
}

export function KeyShortcutRecorder({
  value,
  onChange,
  onClear,
  label = "Hotkey",
  disabled = false,
}: Props) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fieldRef = useRef<HTMLButtonElement | null>(null);

  const handleKeyDown = useCallback(
    async (event: KeyboardEvent) => {
      if (!recording) return;
      // Always swallow the keystroke while recording so we don't
      // accidentally trigger app shortcuts.
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecording(false);
        setError(null);
        return;
      }
      // Ignore standalone modifier presses so the user has time to add the key.
      if (
        ["Control", "Shift", "Alt", "Meta", "OS"].includes(event.key)
      ) {
        return;
      }
      const chord = eventToChord(event);
      if (!chord) {
        setError("Add a modifier (Ctrl/Shift/Alt/Win) plus a letter or digit.");
        return;
      }
      try {
        const validated = await invoke<string>("hotkey_test_chord", { chord });
        setRecording(false);
        setError(null);
        onChange(validated);
      } catch (err) {
        setError(String(err));
      }
    },
    [recording, onChange],
  );

  useEffect(() => {
    if (!recording) return;
    const fn = (e: KeyboardEvent) => void handleKeyDown(e);
    window.addEventListener("keydown", fn, true);
    return () => window.removeEventListener("keydown", fn, true);
  }, [recording, handleKeyDown]);

  const displayValue = value || "Win+Shift+U";

  return (
    <div className="key-shortcut-recorder">
      <label className="key-shortcut-recorder__label">
        <span className="key-shortcut-recorder__label-text">{label}</span>
        <button
          ref={fieldRef}
          type="button"
          className={
            "key-shortcut-recorder__field" +
            (recording ? " key-shortcut-recorder__field--recording" : "")
          }
          onClick={() => {
            if (disabled) return;
            setRecording((r) => !r);
            setError(null);
          }}
          aria-pressed={recording}
          disabled={disabled}
        >
          {recording ? "Press a chord… (Esc to cancel)" : formatChord(displayValue)}
        </button>
      </label>
      {error && (
        <p className="key-shortcut-recorder__error" role="alert">
          {error}
        </p>
      )}
      <button
        type="button"
        className="key-shortcut-recorder__clear"
        onClick={() => {
          setError(null);
          onClear();
        }}
        disabled={disabled}
      >
        Reset to default
      </button>
    </div>
  );
}

/** Pretty-print a chord by mapping `Control` → `Ctrl`, `Meta` → `Win`. */
function formatChord(chord: string): string {
  return chord
    .split("+")
    .map((part) => MODIFIER_LABELS[part] ?? part)
    .join(" + ");
}
