// Phase 3 D1: dedicated popup entry point. The Vite root still loads
// `src/main.tsx` which now re-exports `PopupShell`, but having an
// `index.tsx` here keeps the popup tree colocated for future routing
// (Preferences window will live alongside it in Phase 8).

export { PopupShell } from "./PopupShell";
