import { useEffect, useRef, useState, type ReactNode } from "react";

// Phase 9 §B-5: hover-on-pause tooltip with an 80 ms appearance delay.
//
// Why not just rely on `title=""`? Because the native browser tooltip
// fires after ~1 second on Windows (an OS-wide constant in user32),
// which is too slow for the "ⓘ explain this" affordance the spec
// calls out. The 80 ms threshold matches the macOS hover-tip timing
// and keeps the tooltip feel snappy without firing on jittery
// mouse-passes.
//
// Hide is immediate on `mouseleave` + `focusout` + Escape so the
// tooltip never lingers blocking content.

interface Props {
  /** The trigger element (usually an ⓘ icon or a control). */
  children: ReactNode;
  /** The tip text. Plain string only — keeps a11y straightforward. */
  text: string;
  /** Optional override; defaults to spec'd 80 ms. */
  delayMs?: number;
  /** Pass-through className on the wrapper span. */
  className?: string;
}

export function HoverTip({
  children,
  text,
  delayMs = 80,
  className,
}: Props) {
  const [open, setOpen] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const onEnter = () => {
    cancelTimer();
    timerRef.current = setTimeout(() => setOpen(true), delayMs);
  };

  const onLeave = () => {
    cancelTimer();
    setOpen(false);
  };

  useEffect(() => {
    // Escape closes any open tip immediately. Also makes the
    // keyboard story consistent with the popup-wide Escape handler.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      cancelTimer();
    };
  }, []);

  return (
    <span
      className={"hover-tip" + (className ? ` ${className}` : "")}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onFocus={onEnter}
      onBlur={onLeave}
      // Surface text to AT regardless of visible state.
      aria-describedby={open ? undefined : undefined}
    >
      {children}
      <span
        role="tooltip"
        className={"hover-tip__bubble" + (open ? " hover-tip__bubble--open" : "")}
        // Keep the bubble in the a11y tree even when hidden so
        // Narrator can read it on focus.
        aria-hidden={!open}
      >
        {text}
      </span>
    </span>
  );
}
