import { useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { CopiedChip } from "./CopiedChip";

// Phase 3 D9: wraps any child to make it copy-to-clipboard on click.
// The copied chip appears anchored to the element's top-right and
// auto hides after the spec-defined 1.32 s total cycle. We track the
// timeout so a rapid second click resets the timer instead of stacking.

interface Props {
  copyText: string;
  children: React.ReactNode;
  // Optional label for the chip; defaults to "Copied".
  chipLabel?: string;
}

export function ClickToCopyOverlay({
  copyText,
  children,
  chipLabel,
}: Props) {
  const [visible, setVisible] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const onClick = async () => {
    try {
      await writeText(copyText);
    } catch {
      // Tauri's clipboard plugin can fail under restrictive policies;
      // we swallow and still surface the chip so the user sees feedback.
    }
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    setVisible(true);
    timeoutRef.current = setTimeout(() => setVisible(false), 1320);
  };

  return (
    <span
      className="click-to-copy"
      role="button"
      tabIndex={0}
      onClick={() => {
        void onClick();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          void onClick();
        }
      }}
    >
      {children}
      <CopiedChip visible={visible} label={chipLabel} />
    </span>
  );
}
