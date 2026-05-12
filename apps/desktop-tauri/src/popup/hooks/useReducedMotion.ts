import { useEffect, useState } from "react";

// Phase 3 E3: returns true when the OS reports the user prefers reduced
// motion. Updates live if the system setting changes while the popup
// stays open, which can happen when the user toggles the Windows 11
// "Animation effects" switch.

export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() =>
    typeof window === "undefined"
      ? false
      : window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  return reduced;
}
