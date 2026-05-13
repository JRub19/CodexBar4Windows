import { useCallback } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

// Cursor's auth flow runs entirely in the browser (Workos AuthKit /
// next-auth). Once the user signs in, the AutoImportCookiesButton
// next to this one harvests the session cookies. This button is just
// the convenience launcher.

const SIGN_IN_URL = "https://cursor.com";

export function CursorLoginButton() {
  const open = useCallback(() => {
    void openUrl(SIGN_IN_URL).catch(() => {});
  }, []);

  return (
    <button
      type="button"
      className="settings-action settings-action--primary"
      onClick={open}
    >
      Open cursor.com to sign in
    </button>
  );
}
